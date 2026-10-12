// Runs before every Vitest file. Files in the node environment have no window.
import { beforeEach } from "vitest";
import { installMatchMedia } from "./media";

if (typeof window !== "undefined") {
  beforeEach(installMatchMedia);
}
