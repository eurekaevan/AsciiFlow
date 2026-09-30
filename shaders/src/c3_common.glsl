// Internal C-3 f32 reference only. Packed RGB scalars: no vec3 struct stride.
layout(set=0, binding=2, std430) buffer FloatA { float a[]; };
layout(set=0, binding=3, std430) buffer FloatB { float b[]; };
// Per pixel: pre-limit RGB, bounded RGB, bit mask (float bits), raw R8 coverage.
// Allocated at full size only when capture !=0. Not a compute intermediate.
layout(set=0, binding=4, std430) buffer Observations { float observation[]; };
const uint C3_OBSERVATION_WORDS=20u;
layout(set=0, binding=5, std430) buffer Diagnostics { uint diagnostic[]; };
layout(push_constant) uniform Params {
    uint width, height, grid_width, grid_height;
    uint atlas_width, atlas_height, color, glyph_count;
    uint capture, mode;
} params;
bool c3_finite(vec3 v) { return !any(isnan(v)) && !any(isinf(v)); }
vec3 c3_read_a(uint i) { return vec3(a[3*i],a[3*i+1],a[3*i+2]); }
vec3 c3_read_b(uint i) { return vec3(b[3*i],b[3*i+1],b[3*i+2]); }
void c3_write_a(uint i, vec3 v) { a[3*i]=v.r; a[3*i+1]=v.g; a[3*i+2]=v.b; }
void c3_write_b(uint i, vec3 v) { b[3*i]=v.r; b[3*i+1]=v.g; b[3*i+2]=v.b; }
