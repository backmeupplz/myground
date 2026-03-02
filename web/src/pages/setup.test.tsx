import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { Setup } from "./setup";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Setup", () => {
  it("renders welcome step initially", () => {
    render(<Setup onComplete={() => {}} />);
    expect(screen.getByText("Welcome to MyGround")).toBeTruthy();
    expect(screen.getByText("Get Started")).toBeTruthy();
  });

  it("shows step indicators", () => {
    render(<Setup onComplete={() => {}} />);
    expect(screen.getByText("Welcome")).toBeTruthy();
    expect(screen.getByText("Account")).toBeTruthy();
    expect(screen.getByText("Storage")).toBeTruthy();
    expect(screen.getByText("Services")).toBeTruthy();
    expect(screen.getByText("Done")).toBeTruthy();
  });

  it("describes the setup steps", () => {
    render(<Setup onComplete={() => {}} />);
    expect(screen.getByText("Create your admin account")).toBeTruthy();
    expect(
      screen.getByText("Choose where to store service data"),
    ).toBeTruthy();
  });
});
