export function selectedFolderValue(parent: string, createChildName?: string) {
  if (!createChildName) return parent;
  const separator = parent.includes("\\") ? "\\" : "/";
  return `${parent.replace(/[\\/]+$/, "")}${separator}${createChildName}`;
}
