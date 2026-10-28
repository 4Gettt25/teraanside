import { describe, expect, it } from "vitest";
import de from "../locales/de.json";
import en from "../locales/en.json";

function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]: [string, unknown]) =>
    typeof v === "object" && v !== null ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

describe("locales", () => {
  it("de and en define the same keys", () => {
    expect(keys(de).sort()).toEqual(keys(en).sort());
  });
});
