param([ValidateSet("generic","cortex-a55")][string]$Cpu="cortex-a55")
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$oldRustFlags=$env:CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS
$env:CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS="-C target-cpu=$Cpu"
$sdk="$env:LOCALAPPDATA/Android/Sdk"
$jdk='C:/Program Files/Android/Android Studio/jbr'
$bt=(Get-ChildItem "$sdk/build-tools" -Directory | Sort-Object Name | Select-Object -Last 1).FullName
$platform=(Get-ChildItem "$sdk/platforms" -Directory | Sort-Object Name | Select-Object -Last 1).FullName
$ndk=(Get-ChildItem "$sdk/ndk" -Directory | Sort-Object Name | Select-Object -Last 1).FullName
$env:CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$ndk/toolchains/llvm/prebuilt/windows-x86_64/bin/armv7a-linux-androideabi28-clang.cmd"
$env:JAVA_HOME=$jdk
Push-Location $root
try {
 New-Item -ItemType Directory -Force out/shaders | Out-Null
 foreach($stage in @('vert','frag')) {
  & "$ndk/shader-tools/windows-x86_64/glslc.exe" -O --target-env=vulkan1.0 "shaders/scene.$stage" -o "out/shaders/scene.$stage.spv"
  if($LASTEXITCODE -ne 0){throw 'Shader compilation failed'}
 }
 cargo build --release --target armv7-linux-androideabi
 if($LASTEXITCODE -ne 0){throw 'Rust build failed'}
 New-Item -ItemType Directory -Force out/vulkan-classes,out/vulkan-dex,out/package/lib/armeabi-v7a | Out-Null
 Copy-Item target/armv7-linux-androideabi/release/libtcl_rust_engine.so out/package/lib/armeabi-v7a/
 & "$bt/aapt2.exe" link -o out/base.apk -I "$platform/android.jar" --manifest android/AndroidManifest.xml
 & "$jdk/bin/javac.exe" -encoding UTF-8 -source 8 -target 8 -cp "$platform/android.jar" -d out/vulkan-classes android/dev/tcl/rustengine/VulkanActivity.java
 if($LASTEXITCODE -ne 0){throw 'Java build failed'}
 $classes=@(Get-ChildItem out/vulkan-classes -Recurse -Filter *.class | ForEach-Object FullName)
 & "$bt/d8.bat" --min-api 28 --lib "$platform/android.jar" --output out/vulkan-dex @classes
 & "$jdk/bin/jar.exe" -u -M -f out/base.apk -C out/vulkan-dex classes.dex -C out/package lib
 & "$bt/zipalign.exe" -f 4 out/base.apk out/aligned.apk
 if(!(Test-Path engine.keystore)){& "$jdk/bin/keytool.exe" -genkeypair -keystore engine.keystore -storepass android -keypass android -alias engine -dname 'CN=Rust 3D Development' -keyalg RSA -validity 10000}
 & "$bt/apksigner.bat" sign --ks engine.keystore --ks-pass pass:android --out Rust3D.apk out/aligned.apk
 if($LASTEXITCODE -ne 0){throw 'Signing failed'}
 Write-Output 'Built Rust3D.apk'
} finally {Pop-Location; $env:CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS=$oldRustFlags}
