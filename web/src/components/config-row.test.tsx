import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { ConfigRow } from "./config-row";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ConfigRow", () => {
  it("renders label", () => {
    render(<ConfigRow label="Host" value="localhost" isPassword={false} />);
    expect(screen.getByText("Host")).toBeTruthy();
  });

  it("shows plain value when not password", () => {
    render(<ConfigRow label="Port" value="5432" isPassword={false} />);
    expect(screen.getByText("5432")).toBeTruthy();
  });

  it("masks value when isPassword is true", () => {
    render(<ConfigRow label="Secret" value="mysecret" isPassword={true} />);
    expect(screen.getByText("\u2022".repeat(8))).toBeTruthy();
    expect(screen.queryByText("mysecret")).toBeNull();
  });

  it("renders Copy button", () => {
    render(<ConfigRow label="Key" value="abc" isPassword={false} />);
    expect(screen.getByText("Copy")).toBeTruthy();
  });
});
