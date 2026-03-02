import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { Field } from "./field";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Field", () => {
  it("renders label", () => {
    render(
      <Field label="Email" type="email" value="" onInput={() => {}} />,
    );
    expect(screen.getByText("Email")).toBeTruthy();
  });

  it("renders input with correct type", () => {
    render(
      <Field label="Password" type="password" value="" onInput={() => {}} />,
    );
    const input = screen.getByDisplayValue("") as HTMLInputElement;
    expect(input.type).toBe("password");
  });

  it("displays value", () => {
    render(
      <Field label="Name" type="text" value="hello" onInput={() => {}} />,
    );
    expect(screen.getByDisplayValue("hello")).toBeTruthy();
  });
});
