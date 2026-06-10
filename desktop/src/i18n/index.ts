import { enUS } from "./en-US";
import { zhCN } from "./zh-CN";

export type Language = "zh-CN" | "en-US";

export const dictionaries = {
  "zh-CN": zhCN,
  "en-US": enUS,
};

export function normalizeLanguage(value: string | null | undefined): Language {
  return value === "en-US" ? "en-US" : "zh-CN";
}
