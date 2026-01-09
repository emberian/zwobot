import { createContext, useContext, type ReactNode } from 'react';
import type { WidgetContext } from './types';

// Create context with undefined default (will be provided by TulipWidgets.render)
const WidgetContextReact = createContext<WidgetContext | undefined>(undefined);

export function WidgetContextProvider({
  ctx,
  children,
}: {
  ctx: WidgetContext;
  children: ReactNode;
}) {
  return (
    <WidgetContextReact.Provider value={ctx}>
      {children}
    </WidgetContextReact.Provider>
  );
}

export function useWidgetContext(): WidgetContext {
  const ctx = useContext(WidgetContextReact);
  if (!ctx) {
    throw new Error('useWidgetContext must be used within a WidgetContextProvider');
  }
  return ctx;
}

// Helper hook for posting interactions
export function useInteraction() {
  const ctx = useWidgetContext();

  return {
    postInteraction: ctx.post_interaction,
    messageId: ctx.message_id,
  };
}
