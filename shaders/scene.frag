#version 450
precision highp float;
precision mediump int;
layout(location=0) in mediump vec3 tint;
layout(location=1) in mediump float fog;
layout(location=0) out vec4 frag;
void main(){frag=vec4(mix(tint,vec3(.57,.74,.78),fog),1);}
