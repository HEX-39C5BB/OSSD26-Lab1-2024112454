#version 450
layout(location = 0) in vec3 position;
layout(location = 1) in vec3 color;
layout(location = 2) in vec2 tile;
layout(location = 3) in vec4 atlas_rect;
layout(location = 0) out vec3 tint;
layout(location = 1) out vec2 grid;
layout(location = 2) flat out vec4 atlas;
layout(push_constant) uniform View { mat4 view_projection; } view;
void main() {
    bool hud = tile.x < 0.0;
    gl_Position = hud ? vec4(position, 1.0) : view.view_projection * vec4(position, 1.0);
    tint = color;
    grid = tile;
    atlas = atlas_rect;
}
