{
  channels,
  campground,
  process-compose-flake,
  ...
}: final: prev: {
  inherit (campground.packages.${final.system}) slidev;
  process-compose = import process-compose-flake.lib {pkgs = final;};
}
