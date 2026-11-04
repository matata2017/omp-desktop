// Interface tests must not depend on the host's system language. Individual
// localization tests can still replace navigator or the stored preference.
const testNavigator = typeof navigator === "undefined" ? {} : navigator;
Object.defineProperty(testNavigator, "language", { configurable: true, value: "en-US" });
if (typeof navigator === "undefined") {
	Object.defineProperty(globalThis, "navigator", { configurable: true, value: testNavigator });
}
