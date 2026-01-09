import { createRoot, type Root } from 'react-dom/client';
import { WidgetContextProvider } from './context';
import type { WidgetContext } from './types';

// Import styles
import './styles/globals.css';

// Component imports (will be added as we build them)
import { Dialogue } from './components/game/Dialogue';
// import { Room } from './components/game/Room';
// import { Combat } from './components/game/Combat';
// import { DiceRoll } from './components/game/DiceRoll';

// Component registry
const components: Record<string, React.ComponentType<any>> = {
  dialogue: Dialogue,
  // room: Room,
  // combat: Combat,
  // diceRoll: DiceRoll,
};

// Track React roots by container element
const roots = new WeakMap<HTMLElement, Root>();

// Track current props for updates
const currentProps = new WeakMap<HTMLElement, { type: string; props: Record<string, unknown> }>();

/**
 * Render a game widget component into a container
 */
function render(
  container: HTMLElement,
  componentType: string,
  props: Record<string, unknown>,
  ctx: WidgetContext
): void {
  const Component = components[componentType];

  if (!Component) {
    console.error(`[TulipWidgets] Unknown component type: ${componentType}`);
    container.innerHTML = `<div style="color: red;">Unknown widget: ${componentType}</div>`;
    return;
  }

  // Get or create React root
  let root = roots.get(container);
  if (!root) {
    root = createRoot(container);
    roots.set(container, root);
  }

  // Store current props for update()
  currentProps.set(container, { type: componentType, props });

  // Render the component wrapped in context provider
  root.render(
    <WidgetContextProvider ctx={ctx}>
      <div className="tulip-widget">
        <Component {...props} />
      </div>
    </WidgetContextProvider>
  );
}

/**
 * Update props for an existing widget without full re-render
 */
function update(
  container: HTMLElement,
  newProps: Record<string, unknown>
): void {
  const root = roots.get(container);
  const current = currentProps.get(container);

  if (!root || !current) {
    console.error('[TulipWidgets] Cannot update: container not initialized');
    return;
  }

  const Component = components[current.type];
  if (!Component) {
    return;
  }

  // Merge props
  const mergedProps = { ...current.props, ...newProps };
  currentProps.set(container, { type: current.type, props: mergedProps });

  // Note: React will handle efficient updates
  // We need the ctx to re-render, but we don't have it here
  // For now, this is a limitation - full updates require calling render() again
  console.warn('[TulipWidgets] update() called - for full updates, use render() with ctx');
}

/**
 * Unmount a widget and clean up React root
 */
function unmount(container: HTMLElement): void {
  const root = roots.get(container);
  if (root) {
    root.unmount();
    roots.delete(container);
    currentProps.delete(container);
  }
}

/**
 * Get list of available component types
 */
function getComponentTypes(): string[] {
  return Object.keys(components);
}

// Export the API
const TulipWidgets = {
  render,
  update,
  unmount,
  getComponentTypes,
};

// Expose globally for freeform widget JS
if (typeof window !== 'undefined') {
  (window as any).TulipWidgets = TulipWidgets;
}

export default TulipWidgets;
export { render, update, unmount, getComponentTypes };

// Re-export types for TypeScript users
export type { WidgetContext, ComponentType, ComponentProps } from './types';
export type {
  DialogueProps,
  RoomProps,
  CombatProps,
  DiceRollProps,
  ResourceBarProps,
  PortraitProps,
} from './types';
