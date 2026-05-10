// Feature: weather-web-frontend, Property 4: Model toggle prevents empty selection
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import { toggleModel } from "../../src/components/model-toggle";

/**
 * Property 4: Model toggle prevents empty selection
 *
 * For any current set of enabled models (non-empty subset of the 5 available models)
 * and for any toggle operation (disabling a specific model), the resulting enabled set
 * SHALL never be empty — the toggle operation SHALL be rejected (returns null) if it
 * would result in zero enabled models.
 *
 * **Validates: Requirements 18.4**
 */

const ALL_MODELS = ["ecmwf", "gfs", "icon", "gem", "bom"] as const;

/** Arbitrary that generates a non-empty subset of the 5 available models */
const nonEmptyModelSet = fc
    .subarray([...ALL_MODELS], { minLength: 1, maxLength: 5 })
    .map((arr) => new Set(arr));

/** Arbitrary that picks one model from the full set of 5 */
const anyModel = fc.constantFrom(...ALL_MODELS);

describe("Property 4: Model toggle prevents empty selection", () => {
    it("toggleModel result is never an empty set", () => {
        fc.assert(
            fc.property(nonEmptyModelSet, anyModel, (currentSet, modelToToggle) => {
                const result = toggleModel(currentSet, modelToToggle);

                // If the result is not null, it must have at least one model
                if (result !== null) {
                    expect(result.size).toBeGreaterThan(0);
                }
            }),
            { numRuns: 100 },
        );
    });

    it("toggling the last remaining model returns null", () => {
        fc.assert(
            fc.property(anyModel, (model) => {
                const singleModelSet = new Set([model]);
                const result = toggleModel(singleModelSet, model);

                // Toggling the only enabled model must be rejected (null)
                expect(result).toBeNull();
            }),
            { numRuns: 100 },
        );
    });
});
