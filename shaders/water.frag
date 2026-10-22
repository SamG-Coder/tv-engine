#version 450
#extension GL_GOOGLE_include_directive : require
precision mediump float;
precision highp int;
#include "water_common.glsl"
layout(set=0,binding=5) uniform sampler2DArray waves;
layout(set=0,binding=6) uniform sampler2D lightMap;
layout(set=0,binding=7) uniform sampler2D bedMap;
layout(set=0,binding=8) uniform sampler2D skyMap;
vec3 cachedSky(vec3 d){return textureLod(skyMap,d.xz/(1.+max(0.,d.y))*.5+.5,0).rgb;}
layout(push_constant) uniform Params { highp vec4 eyeTime; vec4 forwardAspect; vec4 rightDepth; vec4 upExposure; } pc;
layout(location=0) in vec3 rayOut;
layout(location=1) in highp vec3 hitOut;
layout(location=2) in highp vec2 shortUV;
layout(location=3) in highp vec2 longUV;
layout(location=4) in vec4 jacobian;
layout(location=0) out vec4 color;
void main(){
 highp vec3 ray=normalize(rayOut),sun=sunDir(),col;
 if(ray.y>=-.002){col=cachedSky(ray);}else{
  highp float t=length(hitOut-pc.eyeTime.xyz),detail=1./(1.+t*t*.0008);
  vec3 a=texture(waves,vec3(shortUV+.5/128.,0)).xyz,b=texture(waves,vec3(longUV+.5/128.,1)).xyz;
  vec2 slope=vec2(dot(a.yz,jacobian.xy),dot(a.yz,jacobian.zw))*detail+b.yz;
  highp vec3 n=normalize(vec3(-slope.x,1,-slope.y));highp float cosine=dot(n,ray),nv=max(.02,-cosine),g=1.-clamp(nv,0.,1.);
  float fresnel=.02037+.97963*g*g*g*g*g;
  vec3 reflected=cachedSky(reflect(ray,n)),transmitted=refract_cosine(ray,n,.7502,cosine);
  float travel=max(0.,(-pc.rightDepth.w-hitOut.y)/min(-.1,transmitted.y));highp vec2 bed=hitOut.xz+transmitted.xz*travel;
  vec3 sand=texture(bedMap,bed/24.).rgb;
  // The same short-wave warp is reused locally across the shallow refracted ray.
  float ca=texture(lightMap,shortUV+(bed-hitOut.xz)/6.).r;ca=mix(1.,ca,detail);
  vec3 attenuation=exp(-(travel+pc.rightDepth.w/.86)*vec3(.19,.09,.055));
  vec3 through=sand*(.12+.95*ca)*attenuation+vec3(.008,.042,.075)*(1.-attenuation);
  col=mix(through,reflected,fresnel);
  highp float footprint=max(length(dFdx(hitOut)),length(dFdy(hitOut))),power=mix(320.,8000.,1./(1.+footprint*footprint*800.));
  highp float spec=pow(max(0.,dot(n,normalize(sun-ray))),power)*3.5*(power/8000.);
  col+=vec3(1,.89,.68)*spec;col=mix(col,vec3(.38,.55,.68),1.-exp(-t*.00025));
 }
 col*=pc.upExposure.w;col=clamp((col*(2.51*col+.03))/(col*(2.43*col+.59)+.14),0.,1.);
 col=pow(col,vec3(.454545));
 // Sub-LSB blue-noise-like dither breaks 8-bit sky bands without a texture fetch.
 float noise=fract(52.9829189*fract(dot(gl_FragCoord.xy,vec2(.06711056,.00583715))));
 color=vec4(col+(noise-.5)/255.,1);
}
