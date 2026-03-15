localFlake:

{
  lib,
  config,
  self,
  inputs,
  ...
}:
{
  perSystem =
    { system, pkgs, ... }:
    {
      packages =
        let
          builder = import ./tracexec-package.nix {
            inherit lib;
            inherit (localFlake) crane;
            inherit pkgs;
          };
        in
        {
          tracexec = builder { };
        };
    };
}
