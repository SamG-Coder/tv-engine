package dev.tcl.rustengine;
import android.app.Activity;
import android.os.Bundle;
import android.view.*;
import android.util.Log;

/** Android SurfaceView and input adapter; Vulkan rendering runs on one native worker. */
public class VulkanActivity extends Activity implements SurfaceHolder.Callback {
 static {System.loadLibrary("tcl_rust_engine");}
 private static native void runVulkan(Surface surface,int objects,int frames,boolean tour,float fixedTime);
 private static native void prepareVulkan();
 private static native void stopVulkan();
 private static native void inputVulkan(int keys,float turn,float move);
 private Thread worker; private SurfaceView view; private boolean ready,resumed,restartPending; private int keys;
 @Override public void onCreate(Bundle b){super.onCreate(b);
  getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
  getWindow().getDecorView().setSystemUiVisibility(5894);
  view=new SurfaceView(this);view.getHolder().setFixedSize(3840,2160);view.getHolder().addCallback(this);
  android.widget.FrameLayout root=new android.widget.FrameLayout(this);root.addView(view);
  if(!getIntent().getBooleanExtra("tour",false)){
   android.widget.TextView help=new android.widget.TextView(this);help.setText("RUST 3D  /  VULKAN 4K     Arrows: walk and turn     OK: boost     Back: exit");help.setTextColor(0xffd2ecdf);help.setTextSize(14);help.setPadding(28,14,28,14);help.setBackgroundColor(0x700b1924);root.addView(help,new android.widget.FrameLayout.LayoutParams(-1,-2,Gravity.BOTTOM));
  }
  setContentView(root);
 }
 private void start(){
  if(!ready||!resumed)return;
  if(worker!=null&&worker.isAlive()){restartPending=true;return;}
  restartPending=false;
  prepareVulkan();
  final Surface surface=view.getHolder().getSurface();
  worker=new Thread(()->{runVulkan(surface,getIntent().getIntExtra("objects",0),getIntent().getIntExtra("frames",0),getIntent().getBooleanExtra("tour",false),getIntent().getFloatExtra("benchmarkTime",-1));runOnUiThread(()->{worker=null;if(restartPending)start();});},"RustVulkan");worker.start();
 }
 @Override public void onResume(){super.onResume();resumed=true;start();}
 @Override public void onPause(){resumed=false;keys=0;inputVulkan(0,0,0);stopVulkan();super.onPause();}
 public void surfaceCreated(SurfaceHolder holder){}
 public void surfaceChanged(SurfaceHolder holder,int format,int w,int h){ready=true;Log.i("Rust3D","Vulkan Surface="+w+"x"+h);start();}
 public void surfaceDestroyed(SurfaceHolder holder){ready=false;stopVulkan();if(worker!=null){try{worker.join(2500);}catch(InterruptedException e){Thread.currentThread().interrupt();}}}
 @Override public boolean dispatchKeyEvent(KeyEvent e){int bit=0;switch(e.getKeyCode()){case 19:bit=1;break;case 20:bit=2;break;case 21:bit=4;break;case 22:bit=8;break;case 23:case 96:bit=16;break;}if(bit!=0){if(e.getAction()==0)keys|=bit;else keys&=~bit;inputVulkan(keys,0,0);return true;}return super.dispatchKeyEvent(e);}
 @Override public boolean onGenericMotionEvent(MotionEvent e){if((e.getSource()&InputDevice.SOURCE_JOYSTICK)!=0){inputVulkan(keys,e.getAxisValue(MotionEvent.AXIS_X),-e.getAxisValue(MotionEvent.AXIS_Y));return true;}return super.onGenericMotionEvent(e);}
}
