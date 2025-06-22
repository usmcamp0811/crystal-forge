{
  channels,
  campground,
  naersk,
  ...
}: final: prev: {
  inherit (campground.packages.${final.system}) slidev;
  naersk-lib = naersk.lib.${final.system};
  process-compose = inputs.process-compose-flake.packages.${final.system}.process-compose;
}
