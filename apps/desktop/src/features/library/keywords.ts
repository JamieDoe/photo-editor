import type { KeywordUseDto } from "../../ipc/generated/KeywordUseDto";

/**
 * Keywords (ADR 0088) as typed: several at once, separated by commas (or semicolons
 * or new lines), as in Lightroom. The catalogue tidies and stores them; this only
 * splits what was typed and suggests the library's own.
 */

/** The longest keyword, in characters (the catalogue's `MAX_KEYWORD_CHARS`). */
export const MAX_KEYWORD_CHARS = 100;

const SEPARATORS = /[,;\n]/;

/** A keyword as the catalogue stores it: trimmed, runs of spaces made one. */
export const tidyKeyword = (text: string) => text.trim().replace(/\s+/g, " ");

/** The keywords in `text`, each once (ignoring case), in order; too long ones left out. */
export function splitKeywords(text: string): string[] {
  const out: string[] = [];
  for (const part of text.split(SEPARATORS)) {
    const name = tidyKeyword(part);
    if (!name || [...name].length > MAX_KEYWORD_CHARS) continue;
    if (!out.some((o) => o.toLowerCase() === name.toLowerCase())) out.push(name);
  }
  return out;
}

/** The keyword being typed: the text after the last separator. */
export const typedFragment = (text: string) => tidyKeyword(text.split(SEPARATORS).pop() ?? "");

/** `text` with the keyword being typed replaced by `name`. */
export function completeFragment(text: string, name: string): string {
  const parts = text.split(SEPARATORS);
  parts[parts.length - 1] = parts.length > 1 ? ` ${name}` : name;
  return parts.join(",");
}

/**
 * The library's keywords that complete what is being typed: those starting with it
 * first, then those containing it, the most used first within each; none on every one
 * of the photos already (`on`, out of `photos`).
 */
export function keywordSuggestions(
  library: readonly KeywordUseDto[],
  text: string,
  on: readonly KeywordUseDto[],
  photos: number,
  limit = 6,
): string[] {
  const typed = typedFragment(text).toLowerCase();
  if (!typed) return [];
  const everywhere = new Set(on.filter((k) => k.photos >= photos).map((k) => k.name.toLowerCase()));
  const rank = (k: KeywordUseDto) => {
    const name = k.name.toLowerCase();
    return name.startsWith(typed) ? 0 : name.includes(typed) ? 1 : -1;
  };
  return library
    .filter((k) => rank(k) >= 0 && !everywhere.has(k.name.toLowerCase()) && k.name.toLowerCase() !== typed)
    .sort((a, b) => rank(a) - rank(b) || b.photos - a.photos || a.name.localeCompare(b.name))
    .slice(0, limit)
    .map((k) => k.name);
}
