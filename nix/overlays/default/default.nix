{
  channels,
  campground,
  ...
}: final: prev: {
  inherit (campground.packages.${final.system}) slidev;
}
