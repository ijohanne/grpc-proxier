# grpc-proxier

A generic gRPC proxy that adds per-user authentication (Basic auth with argon2 password hashes) and per-call authorization to any gRPC server. No `.proto` compilation needed — operates on raw HTTP/2 frames.

```
gRPC Client → [grpc-proxier (auth + authz + metrics)] → gRPC Server
```

## Quick Start

```bash
# Set up environment
cp .env.example .env
cp credentials.example credentials

# Edit credentials with real argon2 hashes (see "Generating Passwords" below)
$EDITOR credentials

# Run the proxy
just run
```

## Building

```bash
# With Nix
nix build

# With Cargo
cargo build --release
```

## Configuration

### Config File (TOML)

Defines listen/upstream addresses and per-user call authorization:

```toml
listen_address = "127.0.0.1:50051"
upstream_address = "127.0.0.1:50052"
metrics_address = "0.0.0.0:9090"

[users.alice]
allowed_calls = [
  "mypackage.MyService/GetStatus",
  "mypackage.MyService/ListItems",
]

[users.bob]
allowed_calls = ["*"]  # wildcard = all calls allowed
```

### Credentials File

One `username:argon2_hash` per line. Lines starting with `#` are comments.

```
alice:$argon2id$v=19$m=19456,t=2,p=1$...
bob:$argon2id$v=19$m=19456,t=2,p=1$...
```

### Generating Passwords

Use `just hash-password` or write a small Rust program:

```rust
use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::Argon2;

fn main() {
    let password = std::env::args().nth(1).expect("usage: hash_password <password>");
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hashing failed");
    println!("{hash}");
}
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `CONFIG_PATH` | Path to the TOML config file |
| `CREDENTIALS_FILE` | Path to the credentials file (not required when `NO_AUTH=1`) |
| `NO_AUTH` | Set to `1` or `true` to disable authentication entirely (passthrough mode) |
| `RUST_LOG` | Log level (`info`, `debug`, `trace`, etc.) |

## NixOS Deployment

### Import the flake module

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    grpc-proxier.url = "github:ijohanne/grpc-proxier";
    sops-nix.url = "github:Mic92/sops-nix"; # optional, for secret management
  };

  outputs = { self, nixpkgs, grpc-proxier, sops-nix, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        grpc-proxier.nixosModules.default
        sops-nix.nixosModules.sops
        ./configuration.nix
      ];
    };
  };
}
```

### Instance configuration

Users and their allowed calls are defined directly in Nix. The module generates the TOML config and credentials file automatically.

```nix
{
  services.grpc-proxier.instances = {
    production = {
      listenPort = 50051;
      upstreamAddress = "10.0.0.5:50052";
      metricsAddress = "0.0.0.0"; # default: 127.0.0.1
      metricsPort = 9090;
      prometheusLabels = { environment = "production"; };

      users = {
        alice = {
          allowedCalls = [
            "mypackage.MyService/GetStatus"
            "mypackage.MyService/ListItems"
          ];
          passwordHashFile = config.sops.secrets.grpc-proxier-alice.path;
        };
        bob = {
          allowedCalls = [ "*" ];
          passwordHashFile = config.sops.secrets.grpc-proxier-bob.path;
        };
      };
    };

    staging = {
      listenPort = 50061;
      upstreamAddress = "10.0.0.6:50052";
      metricsPort = 9091;
      noAuth = true; # passthrough mode for testing
    };
  };
}
```

Each instance gets a hardened systemd service (`grpc-proxier-<name>`).

### Credentials with SOPS

Password hashes are secrets and should be managed with [sops-nix](https://github.com/Mic92/sops-nix). Each user's `passwordHashFile` points to a sops-decrypted file containing just the raw argon2 hash.

#### 1. Generate password hashes

```bash
just hash-password mysecretpassword
# outputs: $argon2id$v=19$m=19456,t=2,p=1$abc123.../def456...
```

#### 2. Create the sops secrets file

```bash
sops secrets/grpc-proxier.yaml
```

```yaml
alice_hash: "$argon2id$v=19$m=19456,t=2,p=1$..."
bob_hash: "$argon2id$v=19$m=19456,t=2,p=1$..."
```

#### 3. Reference in NixOS config

```nix
{ config, ... }:
{
  sops.secrets.grpc-proxier-alice = {
    sopsFile = ./secrets/grpc-proxier.yaml;
    key = "alice_hash";
    owner = "grpc-proxier";
  };

  sops.secrets.grpc-proxier-bob = {
    sopsFile = ./secrets/grpc-proxier.yaml;
    key = "bob_hash";
    owner = "grpc-proxier";
  };

  services.grpc-proxier.instances.production = {
    listenPort = 50051;
    upstreamAddress = "10.0.0.5:50052";

    users = {
      alice = {
        allowedCalls = [
          "mypackage.MyService/GetStatus"
          "mypackage.MyService/ListItems"
        ];
        passwordHashFile = config.sops.secrets.grpc-proxier-alice.path;
      };
      bob = {
        allowedCalls = [ "*" ];
        passwordHashFile = config.sops.secrets.grpc-proxier-bob.path;
      };
    };
  };
}
```

At service start, the module assembles a credentials file from each user's decrypted hash file automatically.

#### Alternative: single credentials file

If you prefer to manage all credentials in one file (`username:argon2_hash` per line), encrypt it as a binary sops secret:

```bash
echo "alice:\$argon2id\$v=19\$m=19456,t=2,p=1\$..." > credentials-production
echo "bob:\$argon2id\$v=19\$m=19456,t=2,p=1\$..." >> credentials-production
sops -e credentials-production > secrets/grpc-proxier-production.enc
```

```nix
{ config, ... }:
{
  sops.secrets.grpc-proxier-production = {
    sopsFile = ./secrets/grpc-proxier-production.enc;
    format = "binary";
    owner = "grpc-proxier";
  };

  services.grpc-proxier.instances.production = {
    listenPort = 50051;
    upstreamAddress = "10.0.0.5:50052";
    credentialsFile = config.sops.secrets.grpc-proxier-production.path;
    users.alice.allowedCalls = [ "*" ];
    users.bob.allowedCalls = [ "mypackage.MyService/GetStatus" ];
  };
}
```

#### No authentication (passthrough mode)

```nix
{
  services.grpc-proxier.instances.testing = {
    listenPort = 50051;
    upstreamAddress = "10.0.0.5:50052";
    noAuth = true;
  };
}
```

### nginx TLS termination

Each instance can optionally use nginx as a TLS-terminating reverse proxy. This keeps the Rust binary simple (plaintext HTTP/2) while exposing a secure endpoint to clients:

```
gRPC Client --h2+TLS--> nginx:443 --h2c--> grpc-proxier --h2c--> gRPC Server
```

#### With ACME (Let's Encrypt)

```nix
{
  # ACME must be configured at the host level
  security.acme = {
    acceptTerms = true;
    defaults.email = "admin@example.com";
  };

  services.grpc-proxier.instances.production = {
    # ... users and credentials as above

    nginx = {
      enable = true;
      domain = "grpc.example.com";
      # acme = true;  # default — uses Let's Encrypt
    };
  };
}
```

#### Without SSL (plain HTTP/2 through nginx)

Useful when TLS is handled by an external load balancer or for internal networks:

```nix
{
  services.grpc-proxier.instances.internal = {
    listenPort = 50051;
    upstreamAddress = "10.0.0.5:50052";
    noAuth = true;

    nginx = {
      enable = true;
      domain = "grpc-internal.example.com";
      acme = false;  # no SSL — plain HTTP/2
    };
  };
}
```

#### nginx options reference

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `nginx.enable` | bool | `false` | Enable nginx in front of this instance |
| `nginx.domain` | string | — | Domain name for the virtual host |
| `nginx.port` | port | `443` | Listen port for nginx |
| `nginx.acme` | bool | `true` | Enable ACME certificates (requires host-level `security.acme` config) |

When nginx is enabled, it automatically forwards `Authorization` headers, `X-Real-IP`, `X-Forwarded-For`, and `X-Forwarded-Proto` to the proxy.

## Monitoring

### Enable monitoring

```nix
{
  services.grpc-proxier.monitoring = {
    enable = true;
    provisionGrafanaDashboard = true; # optional, requires Grafana on the host
  };
}
```

Scrape targets are automatically added to `services.prometheus.scrapeConfigs` when Prometheus is enabled on the host. The monitoring module never enables Prometheus itself.

### Metrics

The proxy exposes Prometheus metrics on a separate HTTP endpoint:

| Metric | Type | Labels |
|--------|------|--------|
| `grpc_proxier_requests_total` | Counter | `user`, `grpc_service`, `grpc_method`, `grpc_status` |
| `grpc_proxier_request_duration_seconds` | Histogram | — |
| `grpc_proxier_auth_failures_total` | Counter | `reason` |
| `grpc_proxier_upstream_errors_total` | Counter | — |
| `grpc_proxier_active_connections` | Gauge | — |

## Development

```bash
# Enter dev shell
nix develop

# Hot-reload development
just dev

# Run lints
just clippy

# Format
just fmt
```

## Testing with grpcurl

```bash
# Encode credentials
CREDS=$(echo -n "alice:mypassword" | base64)

# Make a gRPC call through the proxy
grpcurl -plaintext \
  -H "authorization: Basic $CREDS" \
  localhost:50051 \
  mypackage.MyService/GetStatus
```
