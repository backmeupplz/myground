import { type BackupJobWithApp } from "../api";
import { describeCron } from "./cron";

export const SCHEDULE_PRESETS = [
  { value: "", label: "Manual only" },
  { value: "daily", label: "Daily (2 AM UTC)" },
  { value: "weekly", label: "Weekly (Sun 2 AM UTC)" },
  { value: "monthly", label: "Monthly (1st, 2 AM UTC)" },
  { value: "custom", label: "Custom (cron)" },
];

export function scheduleLabel(schedule?: string): string {
  if (!schedule) return "Manual";
  const preset = SCHEDULE_PRESETS.find((p) => p.value === schedule);
  if (preset) return preset.label;
  const desc = describeCron(schedule);
  return desc || schedule;
}

export function statusBadge(job: BackupJobWithApp): { text: string; color: string } {
  if (job.last_status === "succeeded") return { text: "Succeeded", color: "text-green-400" };
  if (job.last_status === "failed") return { text: "Failed", color: "text-red-400" };
  if (job.last_status === "cancelled") return { text: "Cancelled", color: "text-amber-400" };
  return { text: "Never run", color: "text-gray-500" };
}

export function destBadge(job: BackupJobWithApp): { text: string; color: string } {
  if (job.destination_type === "local") return { text: "Local", color: "bg-blue-900/50 text-blue-400" };
  return { text: "S3", color: "bg-amber-900/50 text-amber-400" };
}
