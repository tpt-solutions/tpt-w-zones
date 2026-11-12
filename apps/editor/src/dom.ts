// Tiny DOM helper. Text is always set with textContent, never innerHTML.

type Child = Node | string | null | undefined | false;

function asText(value: unknown): string {
  return typeof value === "string" || typeof value === "number" || typeof value === "boolean"
    ? String(value)
    : "";
}

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Record<string, unknown> = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) {
      continue;
    }
    if (key === "className") {
      el.className = asText(value);
    } else if (key === "text") {
      el.textContent = asText(value);
    } else if (key.startsWith("on") && typeof value === "function") {
      el.addEventListener(key.slice(2), value as EventListener);
    } else if (value === true) {
      el.setAttribute(key, "");
    } else {
      el.setAttribute(key, asText(value));
    }
  }
  for (const child of children) {
    if (child === null || child === undefined || child === false) {
      continue;
    }
    el.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
  return el;
}

export function byId(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) {
    throw new Error(`missing element #${id}`);
  }
  return el;
}

export function byButton(id: string): HTMLButtonElement {
  return byId(id) as HTMLButtonElement;
}
