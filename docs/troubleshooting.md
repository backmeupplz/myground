# Troubleshooting

## Docker services unreachable when using Tailscale exit node

**Symptoms:** Docker containers start and show as "Running" in MyGround, but
accessing the service on its assigned port hangs indefinitely. TCP connections
establish (via docker-proxy) but never receive a response.

**Cause:** When your machine routes traffic through a Tailscale exit node,
Tailscale adds a default route in its routing table (table 52) that captures
*all* traffic — including packets destined for Docker's bridge subnets
(`172.16.0.0/12`). Instead of reaching the container via `docker0`, packets
are sent through `tailscale0` and never arrive.

You can confirm this is the issue by running:

```sh
ip route get 172.17.0.3
```

If the output shows `dev tailscale0` instead of `dev docker0`, Tailscale is
intercepting Docker bridge traffic.

**Fix:** Add a routing policy rule that forces Docker bridge traffic through the
main routing table (which has the correct `docker0` route) *before* Tailscale's
table is consulted:

```sh
sudo ip rule add from all to 172.16.0.0/12 lookup main priority 5200
```

This inserts a rule at priority 5200, just before Tailscale's rules
(5210–5270). It tells the kernel: "for any packet going to a Docker bridge
subnet, use the main routing table" — which routes through `docker0`.

To verify the fix:

```sh
# Should now show "dev docker0" or "dev br-*"
ip route get 172.17.0.3

# Docker services should respond
curl http://localhost:<service-port>/
```

**Important notes:**

- This rule is **not persistent** across reboots. To make it permanent, add it
  to a systemd service, a NetworkManager dispatcher script, or your system's
  network configuration.
- This only affects machines that are **clients** of a Tailscale exit node. The
  exit node server itself (e.g., a MyGround home server acting as exit node)
  does not route its own local traffic through the tunnel, so this issue does
  not occur there.
- The `172.16.0.0/12` range covers all default Docker bridge subnets
  (`172.17.0.0/16` for the default bridge, `172.18.0.0/16`+ for
  compose-created networks).
- This does not break Tailscale. No legitimate Tailscale traffic uses
  `172.16.0.0/12` — Tailscale's mesh uses the `100.x.x.x` CGNAT range.
