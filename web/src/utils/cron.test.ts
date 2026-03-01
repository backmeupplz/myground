import { describe, it, expect } from "vitest";
import {
  isCustomCron,
  validateCronField,
  validateCron,
  describeCronField,
  describeCron,
} from "./cron";

describe("isCustomCron", () => {
  it("returns false for presets", () => {
    expect(isCustomCron(undefined)).toBe(false);
    expect(isCustomCron("")).toBe(false);
    expect(isCustomCron("daily")).toBe(false);
    expect(isCustomCron("weekly")).toBe(false);
    expect(isCustomCron("monthly")).toBe(false);
  });

  it("returns true for cron expressions", () => {
    expect(isCustomCron("0 2 * * *")).toBe(true);
    expect(isCustomCron("*/5 * * * *")).toBe(true);
  });
});

describe("validateCronField", () => {
  it("accepts wildcard", () => {
    expect(validateCronField("*", 0, 59)).toBe(true);
  });

  it("accepts step values", () => {
    expect(validateCronField("*/5", 0, 59)).toBe(true);
    expect(validateCronField("*/0", 0, 59)).toBe(false);
  });

  it("accepts single values in range", () => {
    expect(validateCronField("0", 0, 59)).toBe(true);
    expect(validateCronField("59", 0, 59)).toBe(true);
    expect(validateCronField("60", 0, 59)).toBe(false);
  });

  it("accepts ranges", () => {
    expect(validateCronField("1-5", 0, 6)).toBe(true);
    expect(validateCronField("5-1", 0, 6)).toBe(false);
  });

  it("accepts comma-separated values", () => {
    expect(validateCronField("1,3,5", 0, 6)).toBe(true);
    expect(validateCronField("0,7", 0, 6)).toBe(false);
  });
});

describe("validateCron", () => {
  it("returns null for valid expressions", () => {
    expect(validateCron("0 2 * * *")).toBeNull();
    expect(validateCron("*/5 * * * *")).toBeNull();
    expect(validateCron("0 0 1 * *")).toBeNull();
    expect(validateCron("30 4 * * 0")).toBeNull();
  });

  it("rejects wrong field count", () => {
    expect(validateCron("0 2 *")).toBe(
      "Must have 5 fields: min hour day month weekday",
    );
  });

  it("rejects invalid minute", () => {
    expect(validateCron("60 2 * * *")).toBe("Invalid minute field (0-59)");
  });

  it("rejects invalid hour", () => {
    expect(validateCron("0 24 * * *")).toBe("Invalid hour field (0-23)");
  });

  it("rejects invalid day", () => {
    expect(validateCron("0 2 0 * *")).toBe("Invalid day field (1-31)");
  });
});

describe("describeCronField", () => {
  it("describes wildcard as every", () => {
    expect(describeCronField("*")).toBe("every");
  });

  it("describes step values", () => {
    expect(describeCronField("*/5")).toBe("every 5");
  });

  it("uses names when provided", () => {
    const days = [
      "Sunday",
      "Monday",
      "Tuesday",
      "Wednesday",
      "Thursday",
      "Friday",
      "Saturday",
    ];
    expect(describeCronField("0", days)).toBe("Sunday");
    expect(describeCronField("1-5", days)).toBe("Monday-Friday");
  });
});

describe("describeCron", () => {
  it("describes daily at 2am", () => {
    const desc = describeCron("0 2 * * *");
    expect(desc).toBe("at 02:00 UTC");
  });

  it("describes every 5 minutes", () => {
    const desc = describeCron("*/5 * * * *");
    expect(desc).toBe("every 5 minutes");
  });

  it("returns null for invalid expressions", () => {
    expect(describeCron("invalid")).toBeNull();
  });
});
