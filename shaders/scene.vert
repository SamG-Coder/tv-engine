#version 450
layout(constant_id=0) const int MODE=0;
layout(location=0) in vec3 position;
layout(location=1) in vec3 color;
layout(location=2) in mat4 model;
layout(location=6) in vec4 instanceTint;
layout(push_constant) uniform Camera {mat4 vp; vec4 eyeTime;} pc;
layout(location=0) out mediump vec3 tint;
layout(location=1) out mediump float fog;
void main(){
 vec3 p=position;
 if(MODE==3)p=(model*vec4(position,1)).xyz;
 if(MODE==2)p+=model[3].xyz;
 float time=pc.eyeTime.w;
 if(MODE==1){p.y+=sin(p.x*.22+time*.7)*.12+sin(p.z*.16-time*.5)*.10;}
 if(MODE==2){p.y+=sin(time*1.5+float(gl_InstanceIndex))*.18;}
 vec4 clip=pc.vp*vec4(p,1);
 // Shared camera uses GLES clip convention; convert to Vulkan Y and depth.
 clip.y=-clip.y;clip.z=(clip.z+clip.w)*.5;gl_Position=clip;
 fog=clamp((length(p-pc.eyeTime.xyz)-26)/65,0,1);
 tint=color;if(MODE==3)tint*=instanceTint.rgb;
 if(MODE==1)tint+=vec3(.02,.04,.04)*sin(p.x*.22+p.z*.12+time*.7);
}
