// Internal BT.2020 NCL/PQ oracle. Linear RGB is normalized to 10,000 cd/m².
// Storage types are narrow; all pixel arithmetic remains uint/f32.
const float PQ_KR = 0.2627;
const float PQ_KB = 0.0593;
const float PQ_KG = 1.0 - PQ_KR - PQ_KB;
const float PQ_M1 = 2610.0 / 16384.0;
const float PQ_M2 = (2523.0 / 4096.0) * 128.0;
const float PQ_C1 = 3424.0 / 4096.0;
const float PQ_C2 = (2413.0 / 4096.0) * 32.0;
const float PQ_C3 = (2392.0 / 4096.0) * 32.0;

struct HdrCell {
    vec4 linear_and_perceptual;
    // glyph, input RGB clamps, invalid samples/components, output clamps.
    uvec4 counts;
};

bool pq_finite(vec3 value) {
    return !any(isnan(value)) && !any(isinf(value));
}

float pq_luminance(vec3 linear) {
    return PQ_KR * linear.r + PQ_KG * linear.g + PQ_KB * linear.b;
}

vec3 pq_eotf(vec3 encoded) {
    vec3 p = pow(encoded, vec3(1.0 / PQ_M2));
    return pow(max(p - PQ_C1, vec3(0.0)) / (PQ_C2 - PQ_C3 * p),
        vec3(1.0 / PQ_M1));
}

vec3 pq_inverse_eotf(vec3 linear) {
    vec3 p = pow(linear, vec3(PQ_M1));
    return pow((PQ_C1 + PQ_C2 * p) / (1.0 + PQ_C3 * p), vec3(PQ_M2));
}

vec3 pq_from_ycbcr(vec3 ycbcr) {
    float r = ycbcr.x + 2.0 * (1.0 - PQ_KR) * ycbcr.z;
    float b = ycbcr.x + 2.0 * (1.0 - PQ_KB) * ycbcr.y;
    return vec3(r, (ycbcr.x - PQ_KR * r - PQ_KB * b) / PQ_KG, b);
}

vec3 pq_to_ycbcr(vec3 encoded) {
    float y = pq_luminance(encoded);
    return vec3(y, (encoded.b - y) / (2.0 * (1.0 - PQ_KB)),
        (encoded.r - y) / (2.0 * (1.0 - PQ_KR)));
}

uint pq_clamp_count(vec3 value, vec3 low, vec3 high) {
    bvec3 outside = bvec3(value.x < low.x || value.x > high.x,
        value.y < low.y || value.y > high.y,
        value.z < low.z || value.z > high.z);
    return uint(outside.x) + uint(outside.y) + uint(outside.z);
}

uvec3 pq_encode_limited(vec3 ycbcr, out uint clamped) {
    vec3 code = vec3(64.0, 512.0, 512.0) + vec3(876.0, 896.0, 896.0) * ycbcr;
    // Legal RGB generates positive codes: Rust round() is floor(code + 0.5).
    vec3 rounded = floor(code + 0.5);
    vec3 low = vec3(64.0);
    vec3 high = vec3(940.0, 960.0, 960.0);
    clamped = pq_clamp_count(rounded, low, high);
    return uvec3(clamp(rounded, low, high));
}

// Inverse of the CPU floor-boundary partition, including uneven cells.
uint pq_cell_at(uint pixel, uint extent, uint count) {
    return ((pixel + 1u) * count - 1u) / extent;
}

uint pq_atlas_at(uint pixel, uint extent, uint count, uint atlas_extent) {
    uint cell = pq_cell_at(pixel, extent, count);
    uint start = cell * extent / count;
    uint end = (cell + 1u) * extent / count;
    return (pixel - start) * atlas_extent / (end - start);
}
