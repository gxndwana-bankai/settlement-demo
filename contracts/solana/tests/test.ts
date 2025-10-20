import * as anchor from "@coral-xyz/anchor";

describe("placeholder", () => {
  anchor.setProvider(anchor.AnchorProvider.env());

  it("noop", async () => {
    // No-op placeholder to avoid failing on missing 'test' program
  });
});
