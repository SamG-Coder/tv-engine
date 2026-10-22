#version 450
#extension GL_GOOGLE_include_directive : require
#include "water_common.glsl"
layout(set=0,binding=5) uniform sampler2DArray waves;
layout(push_constant) uniform Params { vec4 eyeTime; vec4 forwardAspect; vec4 rightDepth; vec4 upExposure; } pc;
layout(location=0) out vec3 rayOut;
layout(location=1) out vec3 hitOut;
layout(location=2) out vec2 shortUV;
layout(location=3) out vec2 longUV;
layout(location=4) out vec4 jacobian;
void main(){
 vec2 uv=vec2(gl_VertexIndex%65,gl_VertexIndex/65)/vec2(64,36);vec2 xy=uv*2.-1.;
 vec3 ray=normalize(pc.forwardAspect.xyz+pc.rightDepth.xyz*xy.x*pc.forwardAspect.w*.65-pc.upExposure.xyz*xy.y*.65);
 rayOut=ray;float t=clamp(-pc.eyeTime.y/min(-.002,ray.y),0.,1500.);
 vec2 origin=pc.eyeTime.xz+ray.xz*t;
 // Cache the smooth regional warp and its Jacobian once per projected vertex.
 // The local linear update avoids sixteen lattice evaluations per vertex.
 vec4 ax=wave_warp(origin.x,origin.y,0,0),az=wave_warp(origin.x,origin.y,0,1),bx=wave_warp(origin.x,origin.y,1,0),bz=wave_warp(origin.x,origin.y,1,1);
 for(int i=0;i<3;i++){
  vec2 p=pc.eyeTime.xz+ray.xz*t,delta=p-origin;
  vec2 small=(p+vec2(ax.x+dot(ax.yz,delta),az.x+dot(az.yz,delta)))/6.;
  vec2 large=(p+vec2(bx.x+dot(bx.yz,delta),bz.x+dot(bz.yz,delta)))/96.;
  float h=textureLod(waves,vec3(small+.5/128.,0),0).x/(1.+t*t*.0008)+textureLod(waves,vec3(large+.5/128.,1),0).x;
  t=mix(t,clamp((h-pc.eyeTime.y)/min(-.002,ray.y),0.,1500.),.75);
 }
 vec3 p=pc.eyeTime.xyz+ray*t;hitOut=p;vec2 delta=p.xz-origin;
 shortUV=(p.xz+vec2(ax.x+dot(ax.yz,delta),az.x+dot(az.yz,delta)))/6.;
 longUV=(p.xz+vec2(bx.x+dot(bx.yz,delta),bz.x+dot(bz.yz,delta)))/96.;
 jacobian=vec4(1.+ax.y,az.y,ax.z,1.+az.z);
 gl_Position=vec4(xy,0,1);
}
