{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  alsa-lib,
  ripgrep,
  chafa,
}:

# 直接用 GitHub Releases 里云端编译好的包，不在本地编译。
# 版本和校验和来自 release.json，由 nix/update-release.py 在发布时更新。
let
  release = lib.importJSON ./release.json;
  system = stdenv.hostPlatform.system;
  platform =
    release.platforms.${system} or (throw "gqy 的预编译包不支持 ${system}，可以改用 .#gqy-src 从源码构建");
  name = "gqy-${platform.target}";
in
stdenv.mkDerivation {
  pname = "gqy";
  inherit (release) version;

  src = fetchurl {
    url = "https://github.com/yxxbc/gqy-agent/releases/download/${release.tag}/${name}.tar.gz";
    inherit (platform) hash;
  };

  # Release 包是在普通 Linux 发行版上编译的，要把动态库路径改到 Nix store 里
  nativeBuildInputs = [ makeWrapper ] ++ lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
    alsa-lib
    stdenv.cc.cc.lib
  ];

  dontConfigure = true;
  dontBuild = true;
  dontStrip = true;

  # 包里本来就是 bin/ share/ lib/ 的前缀布局，原样放进 $out
  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -R bin share $out/
    if [ -d lib ]; then cp -RP lib $out/; fi
    # 文件搜索要 rg，终端看图要 chafa；放在 PATH 末尾，用户自己装的版本优先
    wrapProgram $out/bin/gqy --suffix PATH : ${lib.makeBinPath [ ripgrep chafa ]}
    runHook postInstall
  '';

  meta = {
    description = "顾清影（gqy）命令行 AI 助手（预编译）";
    homepage = "https://github.com/yxxbc/gqy-agent";
    license = import ./license.nix;
    mainProgram = "gqy";
    platforms = builtins.attrNames release.platforms;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
