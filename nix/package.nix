{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  makeWrapper,
  alsa-lib,
  onnxruntime,
  ripgrep,
  chafa,
  buildId ? "nix",
}:

let
  cargoToml = lib.importTOML ../Cargo.toml;
  soExt = stdenv.hostPlatform.extensions.sharedLibrary;
in
rustPlatform.buildRustPackage {
  pname = "gqy";
  inherit (cargoToml.package) version;

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [
    "--bin"
    "gqy"
  ];

  env.GQY_BUILD_ID = buildId;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];
  # rodio 在 Linux 上走 ALSA；macOS 用系统框架，nixpkgs 的默认 SDK 已经带上
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [ alsa-lib ];

  # 测试会碰家目录、起子进程，放在 CI 里跑，不在打包时跑
  doCheck = false;

  postInstall = ''
    share=$out/share/gqy
    licenses=$out/share/licenses/gqy
    mkdir -p $share/fonts $share/models $share/default-kb $licenses $out/lib/gqy

    # 资源，布局和 publish-release.yml 打出的 Release 包一致
    install -m 0644 assets/fonts/NotoSansCJK-Regular.ttc $share/fonts/
    install -m 0644 assets/fonts/NotoColorEmoji.ttf $share/fonts/
    install -m 0644 assets/fonts/JetBrainsMono-Regular.ttf $share/fonts/
    cp -R assets/models/bge-small-zh-v1.5-int8 $share/models/
    cp -R src/scripts $share/scripts
    cp -R kb $share/default-kb/kb

    install -m 0644 LICENSE $licenses/LICENSE
    install -m 0644 assets/fonts/NotoSansCJK.LICENSE $licenses/
    install -m 0644 assets/fonts/NotoColorEmoji.LICENSE $licenses/
    install -m 0644 assets/fonts/JetBrainsMono.LICENSE $licenses/
    install -m 0644 assets/models/bge-small-zh-v1.5-int8/LICENSE $licenses/bge-small-zh-v1.5.LICENSE

    # 本地知识库的向量检索要用 ONNX Runtime，放到程序会找的 lib/gqy 下
    ln -s ${onnxruntime}/lib/libonnxruntime${soExt} $out/lib/gqy/libonnxruntime${soExt}

    # 文件搜索要 rg，终端看图要 chafa；放在 PATH 末尾，用户自己装的版本优先
    wrapProgram $out/bin/gqy --suffix PATH : ${lib.makeBinPath [ ripgrep chafa ]}
  '';

  meta = {
    description = cargoToml.package.description;
    homepage = cargoToml.package.repository;
    license = lib.licenses.mit;
    mainProgram = "gqy";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
