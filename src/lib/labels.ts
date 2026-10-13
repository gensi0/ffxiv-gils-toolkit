// Libellés français des valeurs envoyées par le backend (les valeurs restent en anglais).

export const CONFIDENCE_LABEL: Record<string, string> = {
  HOT: "Très demandé",
  OK: "OK",
  COMPETITIVE: "Concurrentiel",
  SATURATED: "Saturé",
  SLOW: "Lent",
};

export const REGION_LABEL: Record<string, string> = {
  "North-America": "Amérique du Nord",
  Europe: "Europe",
  Oceania: "Océanie",
  Japan: "Japon",
};

/** Libellé français, ou la valeur telle quelle si elle est inconnue. */
export const labelOf = (map: Record<string, string>) => (v: string) => map[v] ?? v;
