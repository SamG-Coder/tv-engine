// Adapted from ClearWater6.1 9b8eca5; see third_party/CLEARWATER6.1-LICENSE.
float clamp01(float a){return min(1,max(0,a));}
float mixf(float a,float b,float t){return a+(b-a)*t;}
vec3 vec(float x,float y,float z){return vec3(x,y,z);}
vec3 plus(vec3 a,vec3 b){return vec(a.x+b.x,a.y+b.y,a.z+b.z);}
vec3 minus(vec3 a,vec3 b){return vec(a.x-b.x,a.y-b.y,a.z-b.z);}
vec3 scale(vec3 a,float b){return vec(a.x*b,a.y*b,a.z*b);}
float dotv(vec3 a,vec3 b){return a.x*b.x+a.y*b.y+a.z*b.z;}
vec3 unit(vec3 a){return scale(a,inversesqrt(max(.0000001,dotv(a,a))));}
vec3 blend(vec3 a,vec3 b,float t){return plus(scale(a,1-t),scale(b,t));}
vec3 crossv(vec3 a,vec3 b){return vec(a.y*b.z-a.z*b.y,a.z*b.x-a.x*b.z,a.x*b.y-a.y*b.x);}
float eased(float a,float b,float x){float t=clamp01((x-a)/(b-a));return t*t*(3-2*t);}
uint scramble(uint a){a^=a>>16;a*=2246822519u;a^=a>>13;a*=3266489917u;a^=a>>16;return a;}
// Scaling these bounded positive floats by 2^-24 is exact in binary32.
float randf(uint a){return (float(scramble(a)&16777215u)+.5)*.000000059604644775390625;}
float cell(float x,float z){return randf(uint(int(x)*92837111+int(z)*689287499));}
vec4 pattern_corners(int ix,int iz,int seed,int period){
 int mask=period-1;
 return vec4(cell((ix&mask)+seed,iz&mask),cell(((ix+1)&mask)+seed,iz&mask),cell((ix&mask)+seed,(iz+1)&mask),cell(((ix+1)&mask)+seed,(iz+1)&mask));
}
vec4 pattern_field(float x,float z,int seed,int period){
 int ix=int(floor(x)),iz=int(floor(z));float u=fract(x),v=fract(z);
 float du=30*u*u*(u-1)*(u-1),dv=30*v*v*(v-1)*(v-1);
 float a=u*u*u*(u*(u*6-15)+10),b=v*v*v*(v*(v*6-15)+10);
 vec4 corners=pattern_corners(ix,iz,seed,period);float p=corners.x,q=corners.y,r=corners.z,t=corners.w;
 return vec4(mixf(mixf(p,q,a),mixf(r,t,a),b),mixf(q-p,t-r,b)*du,mixf(r-p,t-q,a)*dv,0);
}
vec4 wave_warp(float x,float z,int c,int axis){
 float size=c==0?24:384,amplitude=c==0?4.2:67.2;
 vec4 n=pattern_field(x/size,z/size,axis==0?173:719,c==0?256:16);
 return vec4((n.x-.5)*amplitude,n.y*amplitude/size,n.z*amplitude/size,0);
}
vec3 refract_cosine(vec3 d,vec3 n,float eta,float c){return minus(scale(d,eta),scale(n,eta*c+sqrt(max(0,1-eta*eta*(1-c*c)))));}
vec3 refractv(vec3 d,vec3 n,float eta){return refract_cosine(d,n,eta,dotv(d,n));}
vec3 sunDir(){return unit(vec(-.42,.66,-.63));}
float positive_power(float value,float exponent){return value>0?exp2(log2(value)*exponent):0;}
vec3 sky(vec3 d,vec3 sunDirection){
 float v=positive_power(clamp01(d.y),.45);vec3 col=blend(vec(.38,.55,.68),vec(.045,.16,.36),v);
 // Explicit fixed powers avoid the compiler's software-f64 integer pow path.
 float sun=max(0,dotv(d,sunDirection)),s2=sun*sun,s4=s2*s2,s8=s4*s4,s16=s8*s8;
 // Below .99 the disk power is smaller than the minimum float value.
 float sunDisk=sun>.99?positive_power(sun,16000):0;
 col=plus(col,scale(vec(1,.83,.55),sunDisk*20+s16*s16*s16*.07));
 float cloud=clamp01(.5+.25*sin(d.x*28+d.z*17)+.25*sin(d.z*43-d.x*12));
 float cloud2=cloud*cloud,cloud4=cloud2*cloud2;
 float cirrus=cloud4*cloud4*clamp01(d.y*3)*.22;
 return scale(blend(col,vec(.94,.96,1),cirrus),.015+.985*eased(-.08,.3,sunDirection.y));
}
float materialNoise(float x,float z){
 float ix=floor(x),iz=floor(z),u=fract(x),v=fract(z);u=u*u*(3-2*u);v=v*v*(3-2*v);
 return mixf(mixf(cell(ix,iz),cell(ix+1,iz),u),mixf(cell(ix,iz+1),cell(ix+1,iz+1),u),v);
}
vec3 seabed(float x,float z,float footprint){
 float broad=materialNoise(x*.42,z*.42),mid=materialNoise(x*3.1+17,z*3.1);
 float bend=materialNoise(x*.65+13,z*.65-7)*16,branch=materialNoise(x*.31-29,z*.31+11);
 float phase=z*31+x*2.4+bend;
 float ridge=sin(phase),slope=cos(phase);
 float detail=1/(1+footprint*footprint*1100);
 // Subpixel grain and gravel would flicker at reduced screen resolution.
 float grainDetail=1/(1+footprint*footprint*80000);
 float grain=grainDetail>.03?materialNoise(x*170,z*170)*grainDetail:0;
 float illumination=.76+detail*((.045+.12*branch)*ridge-(.04+.13*branch)*slope+.1*grain);
 vec3 sand=scale(blend(vec(.30,.235,.135),vec(.49,.41,.26),broad),illumination*(.85+.2*mid));
 // Sparse organic gravel pockets; most of the floor remains rippled sand.
 float pocket=clamp01((materialNoise(x*.7+83,z*.7-19)-.55)*5);
 if(pocket>.01&&footprint<.075){
  float gx=floor(x*7),gz=floor(z*7),nearest=5,tone=0;
  for(int j=-1;j<=1;j++)for(int i=-1;i<=1;i++){
   float cx=gx+float(i),cz=gz+float(j),seed=cell(cx,cz);
   float px=(cx+cell(cx+23,cz-87))/7,pz=(cz+cell(cx-71,cz+53))/7;
   float dx=(x-px),dz=(z-pz),radius=.012+.04*seed;
   // The largest possible deformed ellipse fits inside this circle.
   // Outside it the original coverage is exactly zero, so skip the expensive
   // angle/edge shading without changing any visible gravel.
   if(dx*dx+dz*dz<=radius*radius*2.34){
   float ang=seed*6.2831853;
   float rx=dx*cos(ang)-dz*sin(ang),rz=dx*sin(ang)+dz*cos(ang);
   float angle=atan(rz,rx);
   float edge=1+.14*sin(angle*5+seed*13)+.09*sin(angle*9);
   float dist=sqrt(rx*rx*1.6+rz*rz*.65)/(radius*edge);
   if(dist<nearest){nearest=dist;tone=seed;}
   }
  }
  float coverage=clamp01((1-nearest)/max(.10,footprint*35))*pocket*clamp01((.075-footprint)/.035);
  vec3 rock=blend(vec(.055,.063,.054),vec(.22,.16,.09),tone);
  rock=scale(rock,.65+.6*sqrt(clamp01(1-nearest*nearest)));
  sand=blend(sand,rock,coverage);
 }
 return sand;
}

