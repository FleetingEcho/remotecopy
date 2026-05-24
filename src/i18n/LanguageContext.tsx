import { createContext, useContext, useMemo } from "react";
import type { Lang, TranslationKey } from "./translations";
import { t as translate } from "./translations";

export interface LanguageContextValue {
  lang: Lang;
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
}

const LanguageContext = createContext<LanguageContextValue>({
  lang: "en",
  t: (key) => translate("en", key),
});

export function useLanguage() {
  return useContext(LanguageContext);
}

export function LanguageProvider({
  lang,
  children,
}: {
  lang: Lang;
  children: React.ReactNode;
}) {
  const value = useMemo<LanguageContextValue>(
    () => ({
      lang,
      t: (key, params) => translate(lang, key, params),
    }),
    [lang],
  );
  return (
    <LanguageContext.Provider value={value}>
      {children}
    </LanguageContext.Provider>
  );
}
