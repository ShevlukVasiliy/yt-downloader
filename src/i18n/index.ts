import { create } from "zustand";
import { ru, type TranslationKey } from "./ru";
import { en } from "./en";
import type { Locale } from "../lib/types";

const dictionaries: Record<Locale, Record<TranslationKey, string>> = { ru, en };

interface LocaleState {
  locale: Locale;
  setLocale: (locale: Locale) => void;
}

export const useLocaleStore = create<LocaleState>((set) => ({
  locale: "ru",
  setLocale: (locale) => set({ locale }),
}));

function format(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key) => String(vars[key] ?? ""));
}

export function translate(
  locale: Locale,
  key: TranslationKey,
  vars?: Record<string, string | number>,
): string {
  const dict = dictionaries[locale];
  return format(dict[key] ?? key, vars);
}

export function useT() {
  const locale = useLocaleStore((s) => s.locale);
  return (key: TranslationKey, vars?: Record<string, string | number>) => translate(locale, key, vars);
}

export type { TranslationKey };
