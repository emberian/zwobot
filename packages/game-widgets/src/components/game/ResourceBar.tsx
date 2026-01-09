import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';
import type { ResourceBarProps } from '@/types';

const colorClasses: Record<string, string> = {
  health: 'tw-bg-health',
  mana: 'tw-bg-mana',
  stamina: 'tw-bg-stamina',
  custom: '',
};

const sizeClasses: Record<string, { bar: string; text: string }> = {
  sm: { bar: 'tw-h-2', text: 'tw-text-xs' },
  md: { bar: 'tw-h-3', text: 'tw-text-sm' },
  lg: { bar: 'tw-h-4', text: 'tw-text-base' },
};

export function ResourceBar({
  label,
  current,
  max,
  color = 'health',
  customColor,
  showNumbers = true,
  size = 'md',
  animated = false,
}: ResourceBarProps) {
  const percentage = Math.min(100, Math.max(0, (current / max) * 100));
  const isLow = percentage <= 25;

  const indicatorColor =
    color === 'custom' && customColor
      ? customColor
      : colorClasses[color] || colorClasses.health;

  return (
    <div className="tw-flex tw-flex-col tw-gap-1">
      {/* Label and numbers row */}
      {(label || showNumbers) && (
        <div
          className={cn(
            'tw-flex tw-justify-between tw-items-center',
            sizeClasses[size].text
          )}
        >
          {label && (
            <span className="tw-font-medium tw-text-foreground">{label}</span>
          )}
          {showNumbers && (
            <span
              className={cn(
                'tw-text-muted-foreground tw-tabular-nums',
                isLow && color === 'health' && 'tw-text-health-low tw-font-medium'
              )}
            >
              {current}/{max}
            </span>
          )}
        </div>
      )}

      {/* Progress bar */}
      <Progress
        value={percentage}
        className={cn(sizeClasses[size].bar, 'tw-bg-muted')}
        indicatorClassName={cn(
          indicatorColor,
          animated && isLow && color === 'health' && 'tw-animate-health-pulse'
        )}
        style={
          color === 'custom' && customColor
            ? ({ '--custom-color': customColor } as React.CSSProperties)
            : undefined
        }
      />
    </div>
  );
}
