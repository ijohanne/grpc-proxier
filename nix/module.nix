{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.grpc-proxier;

  userModule = {
    options = {
      allowedCalls = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        description = ''
          List of gRPC methods this user may call, e.g.
          ["mypackage.MyService/GetStatus"]. Use ["*"] to allow all.
        '';
      };

      passwordHashFile = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = ''
          Path to a file containing the argon2 password hash for this user.
          Useful for per-user sops secrets. Mutually exclusive with
          passwordFile and the instance-level credentialsFile.
        '';
      };

      passwordFile = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = ''
          Path to a file containing the cleartext password for this user.
          The password is hashed with argon2 at service startup.
          Useful with sops when you want to store cleartext passwords
          (encrypted at rest) instead of pre-hashed values.
          Mutually exclusive with passwordHashFile.
        '';
      };
    };
  };

  instanceModule = _: {
    options = {
      listenAddress = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Address to listen on for gRPC connections.";
      };

      listenPort = lib.mkOption {
        type = lib.types.port;
        description = "Port to listen on for gRPC connections.";
      };

      upstreamAddress = lib.mkOption {
        type = lib.types.str;
        description = "Upstream gRPC server address (host:port).";
      };

      metricsAddress = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Address to listen on for the Prometheus metrics endpoint.";
      };

      metricsPort = lib.mkOption {
        type = lib.types.port;
        default = 9090;
        description = "Port for the Prometheus metrics endpoint.";
      };

      users = lib.mkOption {
        type = lib.types.attrsOf (lib.types.submodule userModule);
        default = { };
        description = "Per-user authorization and credential configuration.";
      };

      credentialsFile = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = ''
          Path to a credentials file (username:argon2_hash per line).
          Use this when all credentials live in a single file (e.g. from sops).
          When null, credentials are assembled from each user's passwordHashFile.
        '';
      };

      noAuth = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Disable authentication entirely (passthrough mode).";
      };

      logLevel = lib.mkOption {
        type = lib.types.str;
        default = "info";
        description = "Log level (trace, debug, info, warn, error).";
      };

      prometheusLabels = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = { };
        description = "Extra Prometheus labels for this instance's scrape target.";
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.grpc-proxier;
        description = "The grpc-proxier package to use.";
      };

      nginx = {
        enable = lib.mkEnableOption "nginx TLS termination in front of this grpc-proxier instance";

        domain = lib.mkOption {
          type = lib.types.str;
          description = "Domain name for the nginx virtual host and ACME certificate.";
        };

        port = lib.mkOption {
          type = lib.types.port;
          default = 443;
          description = "Port for the nginx HTTPS listener.";
        };

        acme = lib.mkEnableOption "ACME (Let's Encrypt) certificate for the domain" // {
          default = true;
        };
      };
    };
  };

  enabledInstances = cfg.instances;
  nginxInstances = lib.filterAttrs (_: icfg: icfg.nginx.enable) enabledInstances;

  # Generate the TOML config for an instance entirely from Nix options
  mkInstanceConfig =
    name: icfg:
    let
      usersSections = lib.concatStringsSep "\n" (
        lib.mapAttrsToList (
          username: ucfg:
          let
            calls = lib.concatMapStringsSep ", " (c: ''"${c}"'') ucfg.allowedCalls;
          in
          ''
            [users.${username}]
            allowed_calls = [${calls}]
          ''
        ) icfg.users
      );
    in
    pkgs.writeText "grpc-proxier-${name}.toml" ''
      listen_address = "${icfg.listenAddress}:${toString icfg.listenPort}"
      upstream_address = "${icfg.upstreamAddress}"
      metrics_address = "${icfg.metricsAddress}:${toString icfg.metricsPort}"

      ${usersSections}
    '';

  # Whether a user has any per-user credential source
  hasUserCredential = ucfg: ucfg.passwordHashFile != null || ucfg.passwordFile != null;

  # Assemble a credentials file from per-user passwordHashFile/passwordFile options
  mkCredentialsScript =
    name: icfg:
    let
      users = lib.filterAttrs (_: hasUserCredential) icfg.users;
    in
    pkgs.writeShellScript "grpc-proxier-${name}-credentials" (
      ''
        set -euo pipefail
        CREDS="/run/grpc-proxier/${name}/credentials"
        mkdir -p "$(dirname "$CREDS")"
      ''
      + lib.concatStringsSep "" (
        lib.mapAttrsToList (
          username: ucfg:
          if ucfg.passwordHashFile != null then
            ''
              printf '%s:%s\n' "${username}" "$(cat "${ucfg.passwordHashFile}")" >> "$CREDS"
            ''
          else
            ''
              printf '%s:%s\n' "${username}" "$(cat "${ucfg.passwordFile}" | ${icfg.package}/bin/grpc-proxier-hash)" >> "$CREDS"
            ''
        ) users
      )
      + ''
        echo "$CREDS"
      ''
    );

  # Determine the credentials file path for an instance
  credentialsPath =
    name: icfg:
    if icfg.noAuth then
      null
    else if icfg.credentialsFile != null then
      icfg.credentialsFile
    else
      "/run/grpc-proxier/${name}/credentials";

  # Whether this instance needs a pre-start script to assemble credentials
  needsCredentialsAssembly =
    icfg:
    !icfg.noAuth
    && icfg.credentialsFile == null
    && lib.any hasUserCredential (lib.attrValues icfg.users);
in
{
  options.services.grpc-proxier = {
    instances = lib.mkOption {
      type = lib.types.attrsOf (lib.types.submodule instanceModule);
      default = { };
      description = "Per-instance grpc-proxier configurations.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "grpc-proxier";
      description = "User account under which grpc-proxier runs.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "grpc-proxier";
      description = "Group under which grpc-proxier runs.";
    };
  };

  config = lib.mkIf (builtins.attrNames enabledInstances != [ ]) {
    assertions =
      let
        allListenPorts = lib.mapAttrsToList (_: icfg: icfg.listenPort) enabledInstances;
        allMetricsPorts = lib.mapAttrsToList (_: icfg: icfg.metricsPort) enabledInstances;
        allPorts = allListenPorts ++ allMetricsPorts;
        uniquePorts = lib.unique allPorts;
      in
      [
        {
          assertion = builtins.length allPorts == builtins.length uniquePorts;
          message = "grpc-proxier: all listenPort and metricsPort values must be unique across instances.";
        }
      ]
      ++ lib.mapAttrsToList (name: icfg: {
        assertion =
          icfg.noAuth
          || icfg.credentialsFile != null
          || lib.all hasUserCredential (lib.attrValues icfg.users);
        message = "grpc-proxier.instances.${name}: each user needs a passwordHashFile or passwordFile, or set a credentialsFile, or enable noAuth.";
      }) enabledInstances
      ++ lib.concatLists (
        lib.mapAttrsToList (
          name: icfg:
          lib.mapAttrsToList (username: ucfg: {
            assertion = !(ucfg.passwordHashFile != null && ucfg.passwordFile != null);
            message = "grpc-proxier.instances.${name}.users.${username}: passwordHashFile and passwordFile are mutually exclusive.";
          }) icfg.users
        ) enabledInstances
      );

    users.users.${cfg.user} = {
      isSystemUser = true;
      inherit (cfg) group;
      description = "grpc-proxier service user";
    };
    users.groups.${cfg.group} = { };

    systemd.services = lib.mapAttrs' (
      name: icfg:
      lib.nameValuePair "grpc-proxier-${name}" {
        description = "gRPC Proxier (${name})";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];

        environment = {
          CONFIG_PATH = "${mkInstanceConfig name icfg}";
          RUST_LOG = "${icfg.logLevel},grpc_proxier=${icfg.logLevel}";
        }
        // lib.optionalAttrs icfg.noAuth { NO_AUTH = "1"; }
        // lib.optionalAttrs (!icfg.noAuth) {
          CREDENTIALS_FILE = credentialsPath name icfg;
        };

        serviceConfig = {
          ExecStart = "${icfg.package}/bin/grpc-proxier";
          User = cfg.user;
          Group = cfg.group;
          Restart = "always";
          RestartSec = 5;

          # Security hardening
          NoNewPrivileges = true;
          ProtectSystem = "strict";
          ProtectHome = true;
          PrivateTmp = true;
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectControlGroups = true;
          RestrictNamespaces = true;
          RestrictSUIDSGID = true;
          MemoryDenyWriteExecute = true;
          LockPersonality = true;
        }
        // lib.optionalAttrs (needsCredentialsAssembly icfg) {
          ExecStartPre = "+${mkCredentialsScript name icfg}";
          RuntimeDirectory = "grpc-proxier/${name}";
        };
      }
    ) enabledInstances;

    # nginx virtual hosts for instances with nginx.enable = true
    services.nginx = lib.mkIf (nginxInstances != { }) {
      enable = true;

      # Recommended defaults for gRPC proxying
      recommendedProxySettings = true;
      recommendedTlsSettings = true;

      virtualHosts = lib.mapAttrs' (
        name: icfg:
        let
          useSSL = icfg.nginx.acme;
        in
        lib.nameValuePair "grpc-proxier-${name}" {
          serverName = icfg.nginx.domain;
          listen = [
            {
              addr = "0.0.0.0";
              inherit (icfg.nginx) port;
              ssl = useSSL;
              extraParameters = [ "http2" ];
            }
          ];

          enableACME = lib.mkIf useSSL true;
          forceSSL = lib.mkIf useSSL true;

          # gRPC proxy to the backend instance
          locations."/" = {
            extraConfig = ''
              # gRPC proxying
              grpc_pass grpc://${icfg.listenAddress}:${toString icfg.listenPort};

              # Forward authentication headers
              grpc_set_header Authorization $http_authorization;

              # Forward client identity
              grpc_set_header X-Real-IP $remote_addr;
              grpc_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
              grpc_set_header X-Forwarded-Proto $scheme;

              # gRPC timeouts
              grpc_read_timeout 300s;
              grpc_send_timeout 300s;

              # Allow large messages (gRPC default max is ~4MB)
              client_max_body_size 0;
            '';
          };
        }
      ) nginxInstances;
    };
  };
}
