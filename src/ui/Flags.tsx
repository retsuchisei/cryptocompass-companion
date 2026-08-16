/**
 * Flags for the language switch, drawn rather than typed.
 *
 * Copied from the zones app rather than shared: the two live in separate
 * repositories on purpose and neither imports the other. Two copies of forty
 * lines of paths is a smaller problem than a package between them.
 *
 * Emoji flags were the obvious choice and are the wrong one: Windows ships no
 * glyphs for regional indicator pairs, so a browser there renders the letters
 * "GB" and "RU" in a box instead of a flag - and Windows is what most of this
 * audience plays on. These are a few paths each and render the same
 * everywhere.
 *
 * A flag names a country and not a language, so it is never the whole label:
 * every button carrying one also has the language's own name as its accessible
 * name.
 */

const RATIO = { width: 21, height: 14 };

export function FlagEn() {
  return (
    <svg
      {...RATIO}
      viewBox="0 0 60 40"
      aria-hidden="true"
      class="flag"
    >
      <rect width="60" height="40" fill="#012169" />
      {/* The saltire: white first, then red drawn narrower on top of it. */}
      <path d="M0 0 L60 40 M60 0 L0 40" stroke="#fff" stroke-width="8" />
      <path d="M0 0 L60 40 M60 0 L0 40" stroke="#c8102e" stroke-width="4" />
      {/* The upright cross, over the saltire. */}
      <path d="M30 0 V40 M0 20 H60" stroke="#fff" stroke-width="14" />
      <path d="M30 0 V40 M0 20 H60" stroke="#c8102e" stroke-width="8" />
    </svg>
  );
}

export function FlagRu() {
  return (
    <svg
      {...RATIO}
      viewBox="0 0 60 40"
      aria-hidden="true"
      class="flag"
    >
      <rect width="60" height="13.34" fill="#fff" />
      <rect y="13.34" width="60" height="13.33" fill="#0039a6" />
      <rect y="26.67" width="60" height="13.33" fill="#d52b1e" />
    </svg>
  );
}
