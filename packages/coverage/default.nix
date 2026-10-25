{ pkgs, ... }:
let
  # Profiles and coverage maps must use the LLVM version of the Rust compiler.
  llvm = pkgs.rustc.llvmPackages.llvm;
  collector = pkgs.writeShellApplication {
    name = "coverage-report";
    runtimeInputs = [ llvm pkgs.findutils pkgs.coreutils pkgs.gnugrep pkgs.gawk ];
    text = builtins.readFile ./report.sh;
  };
in
collector // { inherit llvm; }
