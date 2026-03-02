import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { Login } from "./login";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Login", () => {
  it("renders login form", () => {
    render(<Login onLogin={() => {}} />);
    expect(screen.getByText("MyGround")).toBeTruthy();
    expect(screen.getByText("Sign in to continue.")).toBeTruthy();
    expect(screen.getByText("Sign In")).toBeTruthy();
  });

  it("renders username and password fields", () => {
    render(<Login onLogin={() => {}} />);
    expect(screen.getByText("Username")).toBeTruthy();
    expect(screen.getByText("Password")).toBeTruthy();
  });
});
