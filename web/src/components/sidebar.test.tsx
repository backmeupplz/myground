import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { Sidebar } from "./sidebar";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Sidebar", () => {
  it("renders services and settings buttons", () => {
    render(<Sidebar currentPath="/" />);
    expect(screen.getByTitle("Services")).toBeTruthy();
    expect(screen.getByTitle("Settings")).toBeTruthy();
  });

  it("highlights services button on root path", () => {
    render(<Sidebar currentPath="/" />);
    const btn = screen.getByTitle("Services");
    expect(btn.className).toContain("bg-gray-700");
  });

  it("highlights services button on service detail path", () => {
    render(<Sidebar currentPath="/service/whoami" />);
    const btn = screen.getByTitle("Services");
    expect(btn.className).toContain("bg-gray-700");
  });

  it("highlights settings button on settings path", () => {
    render(<Sidebar currentPath="/settings" />);
    const btn = screen.getByTitle("Settings");
    expect(btn.className).toContain("bg-gray-700");
  });

  it("does not highlight services on settings path", () => {
    render(<Sidebar currentPath="/settings" />);
    const btn = screen.getByTitle("Services");
    expect(btn.className).not.toContain("bg-gray-700");
  });
});
