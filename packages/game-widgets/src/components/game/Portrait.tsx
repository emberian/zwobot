import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { cn } from '@/lib/utils';
import type { PortraitProps } from '@/types';

const moodEmojis: Record<string, string> = {
  neutral: '',
  happy: '',
  angry: '',
  sad: '',
  surprised: '',
};

const statusColors: Record<string, string> = {
  friendly: 'tw-ring-friendly',
  neutral: 'tw-ring-neutral',
  hostile: 'tw-ring-hostile',
};

const sizeClasses: Record<string, string> = {
  sm: 'tw-h-8 tw-w-8',
  md: 'tw-h-12 tw-w-12',
  lg: 'tw-h-16 tw-w-16',
};

export function Portrait({
  name,
  imageUrl,
  mood = 'neutral',
  size = 'md',
  status,
}: PortraitProps) {
  // Get initials from name
  const initials = name
    .split(' ')
    .map((n) => n[0])
    .join('')
    .toUpperCase()
    .slice(0, 2);

  return (
    <div className="tw-relative tw-inline-flex tw-flex-col tw-items-center tw-gap-1">
      <Avatar
        className={cn(
          sizeClasses[size],
          status && `tw-ring-2 ${statusColors[status]}`
        )}
      >
        {imageUrl && <AvatarImage src={imageUrl} alt={name} />}
        <AvatarFallback className="tw-text-xs tw-font-medium">
          {initials}
        </AvatarFallback>
      </Avatar>

      {/* Mood indicator */}
      {mood && mood !== 'neutral' && moodEmojis[mood] && (
        <span
          className="tw-absolute -tw-bottom-1 -tw-right-1 tw-text-sm"
          title={mood}
        >
          {moodEmojis[mood]}
        </span>
      )}
    </div>
  );
}
