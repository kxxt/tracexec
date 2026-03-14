let
  pkgs = import <nixpkgs> {};

  bccNoLuaJit = pkgs.bcc.overrideAttrs (old: {
    buildInputs = builtins.filter (pkg: pkg != pkgs.luajit) old.buildInputs;
  });

in pkgs.bpftrace.override {
  bcc = bccNoLuaJit;
}
