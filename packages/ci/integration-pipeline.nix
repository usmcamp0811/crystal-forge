# GitLab child pipeline with one job per registered integration scenario,
# generated from the owners' `scenarios` attribute sets (the only registry):
#   nix eval --json .#checks.x86_64-linux --apply "$(cat packages/ci/integration-pipeline.nix)"
# One stage per owner keeps the grouping; `needs = []` lets scenarios run
# concurrently and be retried individually. JSON is valid GitLab YAML.
checks: let
  inherit (builtins) attrNames concatMap filter listToAttrs match;
  owners = filter (name: match "integration-.*" name != null) (attrNames checks);
  job = owner: scenario: {
    name = "${owner}/${scenario}";
    value = {
      stage = owner;
      tags = ["nix"];
      needs = [];
      interruptible = true;
      # Jobs without `rules` default to branch pipelines only; this child is
      # created by a merge request pipeline, so every job would be dropped.
      rules = [{when = "always";}];
      retry = {
        max = 2;
        when = ["runner_system_failure"];
      };
      before_script = [
        ''echo "machine ''${CI_SERVER_HOST}" > ~/.netrc''
        ''echo "login gitlab-ci-token" >> ~/.netrc''
        ''echo "password ''${CI_JOB_TOKEN}" >> ~/.netrc''
        "chmod 600 ~/.netrc"
      ];
      script = ["sh packages/ci/integration-owner-build.sh ${owner} ${scenario}"];
      artifacts = {
        when = "always";
        expire_in = "30 days";
        paths = ["integration-evidence/"];
      };
    };
  };
in
  {stages = owners;}
  // listToAttrs (concatMap
    (owner: map (job owner) (attrNames checks.${owner}.scenarios))
    owners)
