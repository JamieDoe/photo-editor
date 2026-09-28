type VisibilityCallback = (visible: boolean) => void;

const callbacks = new WeakMap<Element, VisibilityCallback>();
let observer: IntersectionObserver | null = null;

/**
 * Calls `onChange` whenever `el` enters or leaves the visible area (clipped by its
 * scrolling ancestors). One shared observer serves every caller, so long lists stay
 * cheap. Returns a function that stops observing.
 */
export function observeVisibility(el: Element, onChange: VisibilityCallback): () => void {
  observer ??= new IntersectionObserver((entries) => {
    for (const entry of entries) callbacks.get(entry.target)?.(entry.isIntersecting);
  });
  callbacks.set(el, onChange);
  observer.observe(el);
  return () => {
    observer?.unobserve(el);
    callbacks.delete(el);
  };
}
