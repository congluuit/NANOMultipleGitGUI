/** A valid-enough branch name: git itself validates the rest when running `git init -b`. */
export function isPlausibleBranchName(name: string): boolean {
  const n = name.trim();
  return (
    n.length > 0 &&
    !/[\s~^:?*[\\]|\.\.|@\{/.test(n) &&
    !n.startsWith("-") &&
    !n.startsWith("/") &&
    !n.endsWith("/") &&
    !n.endsWith(".lock") &&
    !n.endsWith(".")
  );
}
