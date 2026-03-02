import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { ContainerStatusCard } from "./container-status-card";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ContainerStatusCard", () => {
  it("renders container name and status", () => {
    render(
      <ContainerStatusCard
        container={{ name: "nginx-web", state: "running", status: "Up 2 hours" }}
      />
    );
    expect(screen.getByText("nginx-web")).toBeTruthy();
    expect(screen.getByText("Up 2 hours")).toBeTruthy();
  });

  it("shows check icon for running container", () => {
    render(
      <ContainerStatusCard
        container={{ name: "app", state: "running", status: "Up 5m" }}
      />
    );
    expect(screen.getByText("\u2713")).toBeTruthy();
  });

  it("shows circle icon for exited container", () => {
    render(
      <ContainerStatusCard
        container={{ name: "db", state: "exited", status: "Exited (1) 5m ago" }}
      />
    );
    expect(screen.getByText("\u25cb")).toBeTruthy();
  });

  it("shows check icon for successful exit", () => {
    render(
      <ContainerStatusCard
        container={{ name: "init", state: "exited", status: "Exited (0) 1m ago" }}
      />
    );
    expect(screen.getByText("\u2713")).toBeTruthy();
  });
});
