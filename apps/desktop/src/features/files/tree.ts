/** The repository's files as a folder tree, for the sidebar's Files view. */

export interface Folder {
  /** Full paths of the sub-folders and files, folders and files each sorted by name. */
  folders: string[];
  files: string[];
}

/** Folder path (`""` = repository root) to its content. */
export type Tree = Map<string, Folder>;

export const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

export const parentOf = (path: string) =>
  path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";

/** `path` is `folder` itself or inside it. */
export const isUnder = (path: string, folder: string) =>
  folder === "" || path === folder || path.startsWith(`${folder}/`);

const byName = (a: string, b: string) =>
  baseName(a).localeCompare(baseName(b), undefined, { numeric: true, sensitivity: "base" });

export function buildTree(files: readonly string[], folders: readonly string[] = []): Tree {
  const tree: Tree = new Map([["", { folders: [], files: [] }]]);
  const folder = (path: string): Folder => {
    let node = tree.get(path);
    if (!node) {
      node = { folders: [], files: [] };
      tree.set(path, node);
      folder(parentOf(path)).folders.push(path);
    }
    return node;
  };
  for (const f of folders) folder(f);
  for (const f of files) folder(parentOf(f)).files.push(f);
  for (const node of tree.values()) {
    node.folders.sort(byName);
    node.files.sort(byName);
  }
  return tree;
}

/** Every folder above `path` (closest last), to expand the tree down to it. */
export function ancestors(path: string): string[] {
  const out: string[] = [];
  for (let p = parentOf(path); p; p = parentOf(p)) out.unshift(p);
  return out;
}
