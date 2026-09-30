// Qualification-only float-float packing. Each limb is IEEE float32;
// precise prevents reassociation/contraction in error-free transforms.
layout(local_size_x = 64) in;
layout(set=0,binding=0,std430) readonly buffer InputRgb { float rgb[]; };
layout(set=0,binding=1,std430) writeonly buffer OutputWords { uint words[]; };
layout(set=0,binding=2,std430) buffer Diagnostics { uint counters[]; };
layout(push_constant) uniform Params { uint width; uint height; } p;
vec2 ff_sum(float a, float b) {
    precise float s = a+b;
    precise float v = s-a;
    precise float e = (a-(s-v))+(b-v);
    return vec2(s,e);
}
vec2 ff_add(vec2 a, vec2 b) {
    precise vec2 s = ff_sum(a.x,b.x);
    precise float e = s.y+(a.y+b.y);
    return ff_sum(s.x,e);
}
vec2 ff_neg(vec2 a) { return -a; }
vec2 ff_product(float a, float b) {
    precise float p = a*b;
    precise float ca = 4097.0*a;
    precise float cb = 4097.0*b;
    precise float ah = ca-(ca-a);
    precise float bh = cb-(cb-b);
    precise float al = a-ah;
    precise float bl = b-bh;
    precise float e = (((ah*bh-p)+ah*bl)+al*bh)+al*bl;
    return vec2(p,e);
}
vec2 ff_mul(vec2 a, vec2 b) {
    precise vec2 p = ff_product(a.x,b.x);
    precise float cross = a.x*b.y+a.y*b.x;
    precise float e = p.y+cross;
    return ff_sum(p.x,e);
}
vec3 load_rgb(uint index) { return vec3(rgb[index*3u],rgb[index*3u+1u],rgb[index*3u+2u]); }
bool valid(vec3 c, bool count) {
    bool finite = !any(isnan(c)) && !any(isinf(c));
    bool below = any(lessThan(c,vec3(0.0)));
    bool above = any(greaterThan(c,vec3(1.0)));
    if (count) {
        if (!finite) atomicAdd(counters[0],1u);
        if (below) atomicAdd(counters[1],1u);
        if (above) atomicAdd(counters[2],1u);
    }
    return finite && !below && !above;
}
// High/low splits of the CPU oracle's binary64 decimal constants. Two
// float32 limbs retain about 48 significant bits; this is not binary64.
const vec2 KR = vec2(0.2125999927520752, 7.2479249269008506e-9);
const vec2 KB = vec2(0.0722000002861023, -2.861023085110048e-10);
// Kr/(2*(1-Kb)) and Kb/(2*(1-Kr)), split after binary64 division.
const vec2 CB_CROSS = vec2(0.11457210779190063, -1.734560717281397e-9);
const vec2 CR_CROSS = vec2(0.0458470918238163, -1.2963291551315592e-10);
struct Ncl { vec2 y; vec2 cb; vec2 cr; };
Ncl ncl(vec3 c) {
    precise vec2 rg = ff_sum(c.r,-c.g);
    precise vec2 bg = ff_sum(c.b,-c.g);
    precise vec2 y = ff_add(ff_add(vec2(c.g,0.0),ff_mul(KR,rg)),ff_mul(KB,bg));
    // Cancel each primary chroma coefficient analytically to exactly .5.
    // This is BT.709 NCL in difference form; keeping (B-Y)/1.8556 as
    // separate approximate operations would perturb true half-code ties.
    precise vec2 cb = ff_add(ff_mul(vec2(0.5,0.0),bg),ff_neg(ff_mul(CB_CROSS,rg)));
    precise vec2 cr = ff_add(ff_mul(vec2(0.5,0.0),rg),ff_neg(ff_mul(CR_CROSS,bg)));
    return Ncl(y,cb,cr);
}
uint quantize(vec2 value, float offset, float scale) {
    precise vec2 code = ff_add(vec2(offset,0.0),ff_mul(vec2(scale,0.0),value));
    float base = floor(code.x);
    // Compare the expansion with the exact half-integer; adding .5 to the
    // high limb alone would discard the residual at the decision boundary.
    precise vec2 distance = ff_add(code,vec2(-(base+0.5),0.0));
    bool upper = distance.x > 0.0 || (distance.x == 0.0 && distance.y >= 0.0);
    return uint(base)+(upper ? 1u : 0u);
}
uint sample_code(uint index) {
    uint pixels = p.width*p.height;
    if (index < pixels) {
        vec3 c = load_rgb(index);
        if (!valid(c,true)) return 0u;
        return quantize(ncl(c).y,Y_OFFSET,Y_SCALE) PACK_SHIFT;
    }
    uint uv = index-pixels;
    uint block = uv/2u;
    uint x = (block%(p.width/2u))*2u;
    uint y = (block/(p.width/2u))*2u;
    vec3 a = load_rgb(y*p.width+x);
    vec3 b = load_rgb(y*p.width+x+1u);
    vec3 c = load_rgb((y+1u)*p.width+x);
    vec3 d = load_rgb((y+1u)*p.width+x+1u);
    if (!valid(a,false)||!valid(b,false)||!valid(c,false)||!valid(d,false)) return 0u;
    Ncl na=ncl(a), nb=ncl(b), nc=ncl(c), nd=ncl(d);
    precise vec2 chroma = uv%2u==0u
        ? ff_add(ff_add(ff_add(na.cb,nb.cb),nc.cb),nd.cb)
        : ff_add(ff_add(ff_add(na.cr,nb.cr),nc.cr),nd.cr);
    chroma = ff_mul(chroma,vec2(0.25,0.0));
    return quantize(chroma,C_OFFSET,C_SCALE) PACK_SHIFT;
}
void main() {
    uint word = gl_GlobalInvocationID.x;
    uint samples = p.width*p.height*3u/2u;
    if (word >= (samples+PER_WORD-1u)/PER_WORD) return;
    uint packed = 0u;
    for (uint i=0u;i<PER_WORD;i++) {
        uint index = word*PER_WORD+i;
        if (index < samples) packed |= sample_code(index) << (i*SAMPLE_BITS);
    }
    words[word] = packed;
}
