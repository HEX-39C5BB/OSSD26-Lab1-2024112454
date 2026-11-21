#version 450
layout(location = 0) in vec3 tint;
layout(location = 1) in vec2 grid;
layout(location = 2) flat in vec4 atlas;
layout(set = 0, binding = 0) uniform sampler2D textures;
layout(location = 0) out vec4 color;
void main() {
    if (atlas.z > 0.0) {
        ivec2 pixel = ivec2(atlas.xy + floor(fract(grid) * atlas.zw));
        vec4 material = texelFetch(textures, pixel, 0);
        if (material.a < 0.5) discard;
        color = vec4(tint * material.rgb, 1.0);
        return;
    }
    if (grid.x < 0.0 || grid.y < 0.0) {
        color = vec4(tint, 1.0);
        return;
    }
    vec2 width = max(fwidth(grid), vec2(0.0001));
    vec2 cell = fract(grid);
    vec2 edge = min(cell, 1.0 - cell);
    vec2 lines = 1.0 - smoothstep(width * 0.30, width * 1.25, edge);
    float detail = max(lines.x, lines.y) * (1.0 - smoothstep(0.17, 0.34, max(width.x, width.y)));
    color = vec4(tint * (1.0 - detail * 0.32), 1.0);
}
