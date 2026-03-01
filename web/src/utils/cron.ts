const WEEKDAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

const MONTHS = [
  "",
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export function isCustomCron(schedule: string | undefined): boolean {
  if (!schedule) return false;
  return !["", "daily", "weekly", "monthly"].includes(schedule);
}

export function validateCronField(
  field: string,
  min: number,
  max: number,
): boolean {
  if (field === "*") return true;
  // */N step
  if (/^\*\/\d+$/.test(field)) {
    const step = parseInt(field.slice(2), 10);
    return step >= 1 && step <= max;
  }
  // comma-separated list of values or ranges
  return field.split(",").every((part) => {
    const range = part.split("-");
    if (range.length === 2) {
      const [a, b] = range.map((s) => parseInt(s, 10));
      return !isNaN(a) && !isNaN(b) && a >= min && b <= max && a <= b;
    }
    if (range.length === 1) {
      const n = parseInt(range[0], 10);
      return !isNaN(n) && n >= min && n <= max;
    }
    return false;
  });
}

export function validateCron(expr: string): string | null {
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return "Must have 5 fields: min hour day month weekday";
  const [min, hour, day, month, weekday] = parts;
  if (!validateCronField(min, 0, 59)) return "Invalid minute field (0-59)";
  if (!validateCronField(hour, 0, 23)) return "Invalid hour field (0-23)";
  if (!validateCronField(day, 1, 31)) return "Invalid day field (1-31)";
  if (!validateCronField(month, 1, 12)) return "Invalid month field (1-12)";
  if (!validateCronField(weekday, 0, 6))
    return "Invalid weekday field (0-6, Sun=0)";
  return null;
}

export function describeCronField(
  field: string,
  names?: string[],
): string {
  if (field === "*") return "every";
  if (field.startsWith("*/")) return `every ${field.slice(2)}`;
  const parts = field.split(",").map((p) => {
    if (p.includes("-")) {
      const [a, b] = p.split("-");
      const na = names ? names[parseInt(a, 10)] : a;
      const nb = names ? names[parseInt(b, 10)] : b;
      return `${na}-${nb}`;
    }
    return names ? names[parseInt(p, 10)] : p;
  });
  return parts.join(", ");
}

export function describeCron(expr: string): string | null {
  if (validateCron(expr)) return null;
  const [min, hour, day, month, weekday] = expr.trim().split(/\s+/);

  const parts: string[] = [];

  // Time
  const minDesc =
    min === "*"
      ? "every minute"
      : min.startsWith("*/")
        ? `every ${min.slice(2)} minutes`
        : null;
  const hourDesc =
    hour === "*"
      ? null
      : hour.startsWith("*/")
        ? `every ${hour.slice(2)} hours`
        : null;

  if (minDesc && hour === "*") {
    parts.push(minDesc);
  } else if (
    min !== "*" &&
    hour !== "*" &&
    !min.startsWith("*/") &&
    !hour.startsWith("*/")
  ) {
    const hours = hour.split(",").map((h) => {
      const mins = min.split(",").map((m) => m.padStart(2, "0"));
      return mins.map((m) => `${h.padStart(2, "0")}:${m}`).join(", ");
    });
    parts.push(`at ${hours.join(", ")} UTC`);
  } else {
    if (minDesc) parts.push(`minute: ${minDesc}`);
    else if (min !== "0" && min !== "*") parts.push(`at minute ${min}`);
    if (hourDesc) parts.push(hourDesc);
    else if (hour !== "*" && !parts.some((p) => p.includes(":")))
      parts.push(`at hour ${hour} UTC`);
  }

  // Day of month
  if (day !== "*") {
    const dayDesc = describeCronField(day);
    parts.push(
      day.startsWith("*/") ? `every ${day.slice(2)} days` : `on day ${dayDesc}`,
    );
  }

  // Month
  if (month !== "*") {
    parts.push(`in ${describeCronField(month, MONTHS)}`);
  }

  // Weekday
  if (weekday !== "*") {
    parts.push(`on ${describeCronField(weekday, WEEKDAYS)}`);
  }

  return parts.join(", ") || "every minute";
}
