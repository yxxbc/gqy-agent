{
  # 顾清影（gqy）的 Nix 安装方式。
  #
  #   nix profile install github:yxxbc/gqy-agent/gqy   装进当前用户
  #   nix run github:yxxbc/gqy-agent/gqy               不安装，直接跑一次
  #   nix build .#gqy-src                              在仓库里从源码构建，结果在 ./result
  #
  # 默认包（gqy）直接用 GitHub Releases 里云端编译好的包，按 nix/release.json 里的 sha256 校验，
  # 不在本地编译；发布流程会自动更新 release.json。gqy-src 从源码编译，ONNX Runtime 用 nixpkgs 里的。
  # 两者布局一致：bin/gqy + share/gqy/{fonts,models,scripts,default-kb} + lib/gqy/libonnxruntime，
  # 程序在「二进制所在目录/../share/gqy」下找资源，所以不用额外配置。
  # 语音前端 gqy-voice 依赖 sherpa-onnx 静态库，这里暂不构建。
  description = "顾清影（gqy）命令行 AI 助手";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      # nixpkgs 从 26.11 起不再支持 Intel Mac（x86_64-darwin），那边用 install.sh
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      # gqy 是 PolyForm Noncommercial（禁止商用，见 nix/license.nix），nixpkgs 会把它
      # 当非自由软件拒绝求值。这里导入 nixpkgs 时只放行 gqy 自己，别的包照旧。
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          config.allowUnfreePredicate = pkg: nixpkgs.lib.getName pkg == "gqy";
        };
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (pkgsFor system));
    in
    {
      packages = forAllSystems (pkgs: {
        # 默认：下载 Releases 里云端编译好的包，几秒装完
        gqy = pkgs.callPackage ./nix/prebuilt.nix { };
        # 从源码编译，适合改了代码或想自己编译的人：nix build .#gqy-src
        gqy-src = pkgs.callPackage ./nix/package.nix {
          # 同一个提交构建出同一个 GQY_BUILD_ID；有未提交改动时退回 dirty
          buildId = self.shortRev or self.dirtyShortRev or "dirty";
        };
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.gqy;
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.gqy}/bin/gqy";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.gqy-src ];
          packages = [
            pkgs.cargo
            pkgs.rustc
            pkgs.clippy
            pkgs.rustfmt
            pkgs.rust-analyzer
          ];
          GQY_ONNXRUNTIME_LIB = "${pkgs.onnxruntime}/lib/libonnxruntime${pkgs.stdenv.hostPlatform.extensions.sharedLibrary}";
        };
      });
    };
}
