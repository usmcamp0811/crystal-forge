# Generates the GitLab child pipeline for integration scenarios from the
# authoritative registry: the `scenarios` attribute sets of the integration
# owner checks in `checks.<system>`. Applied by packages/ci/integration-pipeline.sh
# with `nix eval --json .#checks.x86_64-linux --apply`.
#
# Physical model: one job per registered scenario, with `needs: []`, so
# independent scenarios run concurrently on any eligible runner and a failed
# scenario can be retried alone. Logical model: one stage per owner keeps the
# owner grouping; `needs: []` stops stage order from serializing the jobs.
#
# INVARIANT (fail closed): evaluation aborts unless
#   - at least one owner exists and every owner registers at least one
#     scenario;
#   - every owner aggregate links exactly its registered scenarios (the names
#     in `aggregate.entries` equal the names in `scenarios`), so building every
#     generated job is building every gate of every aggregate. Only attribute
#     names are compared. Comparing gate outputs would instantiate every NixOS
#     VM test during generation, which delayed all scenario jobs by minutes on
#     a cold runner. The aggregate links each `scenarios` value by
#     construction, so equal names mean equal gates;
#   - every owner and scenario name is a safe job and attribute name.
# The output is JSON, which GitLab accepts as YAML.
checks: let
  inherit (builtins) attrNames concatMap filter listToAttrs match all;
  owners = filter (name: match "integration-.*" name != null && checks.${name} ? scenarios)
    (attrNames checks);
  safe = name: match "[A-Za-z0-9_-]+" name != null;
  netrc = [
    ''echo "machine ''${CI_SERVER_HOST}" > ~/.netrc''
    ''echo "login gitlab-ci-token" >> ~/.netrc''
    ''echo "password ''${CI_JOB_TOKEN}" >> ~/.netrc''
    "chmod 600 ~/.netrc"
  ];
  check = owner: let
    aggregate = checks.${owner};
    scenarios = aggregate.scenarios;
    names = attrNames scenarios;
    entries = attrNames (aggregate.entries or (throw "integration pipeline: ${owner} aggregate has no entries"));
  in
    if names == []
    then throw "integration pipeline: ${owner} registers no scenarios"
    else if entries != names
    then throw "integration pipeline: ${owner} aggregate entries differ from its scenarios"
    else if !(all safe ([owner] ++ names))
    then throw "integration pipeline: unsafe owner or scenario name in ${owner}"
    else names;
  job = owner: scenario: {
    name = "${owner}/${scenario}";
    value = {
      stage = owner;
      tags = ["nix"];
      needs = [];
      interruptible = true;
      retry = {
        max = 2;
        when = ["runner_system_failure"];
      };
      before_script = netrc;
      script = ["sh packages/ci/integration-owner-build.sh ${owner} ${scenario}"];
      artifacts = {
        when = "always";
        expire_in = "30 days";
        paths = ["integration-evidence/"];
      };
    };
  };
in
  if owners == []
  then throw "integration pipeline: no integration owners found"
  else
    {stages = owners;}
    // listToAttrs (concatMap (owner: map (job owner) (check owner)) owners)
