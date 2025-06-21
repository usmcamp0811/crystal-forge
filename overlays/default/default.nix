{
  channels,
  campground,
  naersk,
  ...
}: final: prev: {
  inherit (campground.packages.${final.system}) slidev;
  naersk-lib = naersk.lib.${final.system};
}
