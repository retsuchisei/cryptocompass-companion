/**
 * The player/organiser split.
 *
 * It is a menu today because there is nothing to authenticate against. When
 * accounts exist the same split becomes a permission rather than a choice, and
 * that moves this boundary instead of carving a new one - which is why it is a
 * union and not a boolean.
 */
export type Mode = { kind: "player" } | { kind: "organiser" };

export const PLAYER: Mode = { kind: "player" };
export const ORGANISER: Mode = { kind: "organiser" };

/** In menu order. */
export const MODES: readonly Mode[] = [PLAYER, ORGANISER];
