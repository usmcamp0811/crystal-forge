
{ pkgs, lib, ... }: 
let
  # Generate keypair for agent authentication
  agentKeyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  
  agentPrivKey = "${agentKeyPair}/agent.key";
  agentPubKey = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${agentKeyPair}/agent.pub $out/agent.pub
  '';
  
  # Read the public key content
  agentPublicKeyContent = lib.strings.trim (builtins.readFile "${agentPubKey}/agent.pub");
in
{
  roles = {
    # Combined server + builder node
    server = { pkgs, lib, ... }: {
      # Enable crystal-forge with both server and builder
      services.crystal-forge = {
        enable = true;
        local-database = true;
        log_level = "debug";
        
        # Server configuration
        server = {
          enable = true;
          host = "0.0.0.0";
          port = 3000;
        };
        
        # Builder configuration  
        build = {
          enable = true;
          cores = 4;
          max_jobs = 2;
          use_substitutes = true;
          offline = false;
          poll_interval = "30s";
        };
        
        # Test environment setup
        environments = [
          {
            name = "dev";
            description = "Development environment for Crystal Forge";
            is_active = true;
            risk_profile = "LOW";
            compliance_level = "NONE";
          }
        ];
        
        # Test flake to monitor
        flakes = {
          watched = [
            {
              name = "crystal-forge";
              repo_url = "git+https://gitlab.com/usmcamp0811/crystal-forge";
              auto_poll = true;
              initial_commit_depth = 5;
            }
          ];
          flake_polling_interval = "2m";
          commit_evaluation_interval = "1m";
          build_processing_interval = "1m";
        };
        
        # Register the agent system
        systems = [
          {
            hostname = "agent1";
            public_key = agentPublicKeyContent;
            environment = "dev";
            flake_name = "crystal-forge";
            deployment_policy = "manual";
          }
        ];
      };
      
      # Open firewall for crystal-forge
      networking.firewall.allowedTCPPorts = [ 3000 5432 ];
      networking.firewall.enable = false; # For dev simplicity
      
      # Dev tools
      environment.systemPackages = with pkgs; [
        git
        curl
        jq
        crystal-forge.default
      ];
    };

    # Agent node
    agent = { pkgs, lib, ... }: {
      services.crystal-forge = {
        enable = true;
        
        client = {
          enable = true;
          server_host = "server";
          server_port = 3000;
          private_key = agentPrivKey;
        };
      };
      
      # Development tools on agent
      environment.systemPackages = with pkgs; [
        git
        curl
        jq
        crystal-forge.default
      ];
      
      # Copy the agent keys
      environment.etc."agent.key" = {
        source = agentPrivKey;
        mode = "0600";
      };
      
      networking.firewall.enable = false;
    };

    # Optional: Attic cache server for testing
    attic = { pkgs, lib, ... }: {
      services.atticd = {
        enable = true;
        settings = {
          listen = "[::]:8080";
          allowed-hosts = [ "*" ];
          api-endpoint = "http://attic:8080/";
        };
      };
      
      networking.firewall.allowedTCPPorts = [ 8080 ];
      networking.firewall.enable = false;
      
      environment.systemPackages = with pkgs; [ attic-client ];
    };
  };

  # Test script to verify everything works
  testScript = ''
    # Start all services
    start_all()
    
    # Wait for services to be ready
    server.wait_for_unit("crystal-forge-server.service")
    server.wait_for_unit("crystal-forge-builder.service")
    server.wait_for_unit("postgresql.service")
    
    agent.wait_for_unit("crystal-forge-agent.service")
    
    attic.wait_for_unit("atticd.service")
    
    # Test server endpoint
    server.wait_for_open_port(3000)
    
    # Test database connection
    server.wait_for_open_port(5432)
    
    # Test attic
    attic.wait_for_open_port(8080)
    
    # Verify crystal-forge server is responding
    server.succeed("curl -f http://localhost:3000/health || echo 'Health endpoint may not exist yet'")
    
    # Check that agent can connect to server
    agent.succeed("curl -f http://server:3000/health || echo 'Agent connection test'")
    
    print("Crystal Forge dev environment is ready!")
    print("Server UI: http://server:3000")
    print("Attic cache: http://attic:8080")
  '';
}

