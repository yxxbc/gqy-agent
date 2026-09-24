# gqy 的许可证：PolyForm Noncommercial 1.0.0（禁止商用），nixpkgs 里没有现成定义。
#
# 非商业许可不算自由软件，`free = false`。nixpkgs 默认拒绝求值非自由的包，所以
# flake.nix 导入 nixpkgs 时只对 gqy 自己放行（allowUnfreePredicate），用户照样
# `nix profile install` 一行装上，不用改自己的 nixpkgs 配置。
{
  shortName = "polyform-nc-100";
  fullName = "PolyForm Noncommercial License 1.0.0";
  spdxId = "PolyForm-Noncommercial-1.0.0";
  url = "https://polyformproject.org/licenses/noncommercial/1.0.0";
  free = false;
  redistributable = true;
}
