#version 450
layout(set=0,binding=9) uniform sampler2D waterImage;
layout(push_constant) uniform Scale { vec2 scale; } pc;
layout(location=0) in vec2 uv;
layout(location=0) out vec4 color;
void main(){vec2 texel=1./vec2(textureSize(waterImage,0));color=textureLod(waterImage,clamp(uv*pc.scale,texel*.5,pc.scale-texel*.5),0);}
