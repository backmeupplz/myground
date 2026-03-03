import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { Sidebar } from "./sidebar";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Sidebar", () => {
  it("renders apps and settings buttons", () => {
    render(<Sidebar currentPath="/" />);
    expect(screen.getByTitle("Apps")).toBeTruthy();
    expect(screen.getByTitle("Settings")).toBeTruthy();
  });

  it("highlights apps button on root path", () => {
    render(<Sidebar currentPath="/" />);
    const btn = screen.getByTitle("Apps");
    expect(btn.className).toContain("bg-gray-700");
  });

  it("highlights apps button on app detail path", () => {
    render(<Sidebar currentPath="/app/whoami" />);
    const btn = screen.getByTitle("Apps");
    expect(btn.className).toContain("bg-gray-700");
  });

  it("highlights settings button on settings path", () => {
    render(<Sidebar currentPath="/settings" />);
    const btn = screen.getByTitle("Settings");
    expect(btn.className).toContain("bg-gray-700");
  });

  it("does not highlight apps on settings path", () => {
    render(<Sidebar currentPath="/settings" />);
    const btn = screen.getByTitle("Apps");
    expect(btn.className).not.toContain("bg-gray-700");
  });
});
