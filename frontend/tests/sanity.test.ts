import { describe, it, expect } from "vitest";

describe("test runner sanity check", () => {
    it("passes a trivial assertion", () => {
        expect(1 + 1).toBe(2);
    });
});
