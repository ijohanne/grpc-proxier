{
  config,
  lib,
  ...
}:
let
  cfg = config.services.grpc-proxier.monitoring;
  proxierCfg = config.services.grpc-proxier;

  dashboardDir = builtins.dirOf cfg.dashboardFile;
in
{
  options.services.grpc-proxier.monitoring = {
    enable = lib.mkEnableOption "grpc-proxier monitoring integration";

    provisionGrafanaDashboard = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Automatically provision the Grafana dashboard if Grafana is enabled on the host.";
    };

    dashboardFile = lib.mkOption {
      type = lib.types.path;
      default = ../../docs/grafana-dashboard.json;
      description = "Path to the Grafana dashboard JSON file.";
    };

    scrapeInterval = lib.mkOption {
      type = lib.types.str;
      default = "15s";
      description = "Prometheus scrape interval.";
    };
  };

  config = lib.mkIf cfg.enable {
    services.grafana.provision =
      lib.mkIf (cfg.provisionGrafanaDashboard && config.services.grafana.enable)
        {
          enable = true;
          dashboards.settings.providers = [
            {
              name = "grpc-proxier";
              options.path = dashboardDir;
            }
          ];
        };

    services.prometheus.scrapeConfigs = lib.mkIf config.services.prometheus.enable [
      {
        job_name = "grpc-proxier";
        scrape_interval = cfg.scrapeInterval;
        metrics_path = "/metrics";
        static_configs = lib.mapAttrsToList (
          _: icfg:
          {
            targets = [ "${icfg.metricsAddress}:${toString icfg.metricsPort}" ];
          }
          // lib.optionalAttrs (icfg.prometheusLabels != { }) { labels = icfg.prometheusLabels; }
        ) proxierCfg.instances;
      }
    ];
  };
}
