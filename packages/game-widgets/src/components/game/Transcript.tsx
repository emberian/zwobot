import { useState, useEffect, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { useWidgetContext } from '@/context';
import { cn } from '@/lib/utils';
import type { TranscriptProps, TranscriptEntry, SubmessageData } from '@/types';
import { Send } from 'lucide-react';

// Type guard for transcript entry submessage content
function isTranscriptEntryContent(
  content: unknown
): content is { type: 'transcript_entry'; speaker: string; text: string; timestamp?: number; avatarUrl?: string; speakerColor?: string } {
  return (
    typeof content === 'object' &&
    content !== null &&
    'type' in content &&
    (content as { type: unknown }).type === 'transcript_entry' &&
    'speaker' in content &&
    'text' in content
  );
}

// Convert submessage to transcript entry
function submessageToEntry(data: SubmessageData): TranscriptEntry | null {
  if (!isTranscriptEntryContent(data.content)) {
    return null;
  }
  return {
    speaker: data.content.speaker,
    text: data.content.text,
    timestamp: data.content.timestamp,
    avatarUrl: data.content.avatarUrl,
    speakerColor: data.content.speakerColor,
  };
}

function getInitials(name: string): string {
  return name
    .split(' ')
    .map((word) => word[0])
    .join('')
    .toUpperCase()
    .slice(0, 2);
}

function formatTimestamp(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function EntryAvatar({ entry }: { entry: TranscriptEntry }) {
  return (
    <Avatar className="tw-h-8 tw-w-8 tw-flex-shrink-0">
      {entry.avatarUrl && <AvatarImage src={entry.avatarUrl} alt={entry.speaker} />}
      <AvatarFallback className="tw-text-xs tw-bg-muted">
        {getInitials(entry.speaker)}
      </AvatarFallback>
    </Avatar>
  );
}

function TranscriptEntryRow({
  entry,
  showTimestamp,
}: {
  entry: TranscriptEntry;
  showTimestamp: boolean;
}) {
  return (
    <div className="tw-flex tw-gap-3 tw-py-2">
      <EntryAvatar entry={entry} />
      <div className="tw-flex-1 tw-min-w-0">
        <div className="tw-flex tw-items-baseline tw-gap-2">
          <span
            className="tw-font-semibold tw-text-sm"
            style={entry.speakerColor ? { color: entry.speakerColor } : undefined}
          >
            {entry.speaker}
          </span>
          {showTimestamp && entry.timestamp && (
            <span className="tw-text-xs tw-text-muted-foreground">
              {formatTimestamp(entry.timestamp)}
            </span>
          )}
        </div>
        <div className="tw-text-sm tw-text-foreground tw-whitespace-pre-wrap">
          {entry.text}
        </div>
      </div>
    </div>
  );
}

export function Transcript({
  title,
  entries: initialEntries,
  inputEnabled = true,
  inputPlaceholder = 'Add to transcript...',
  inputButtonText = 'Send',
  showTimestamps = false,
  maxHeight = 400,
  autoScroll = true,
}: TranscriptProps) {
  const ctx = useWidgetContext();
  const [inputText, setInputText] = useState('');
  const [sending, setSending] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Local state for entries: initialized from props + any initial submessages
  const [entries, setEntries] = useState<TranscriptEntry[]>(() => {
    const fromSubmessages: TranscriptEntry[] = [];
    if (ctx.initial_submessages) {
      for (const sm of ctx.initial_submessages) {
        const entry = submessageToEntry(sm);
        if (entry) {
          fromSubmessages.push(entry);
        }
      }
    }
    return [...initialEntries, ...fromSubmessages];
  });

  // Subscribe to live submessage updates
  useEffect(() => {
    if (!ctx.on_submessage) return;

    ctx.on_submessage((data: SubmessageData) => {
      const entry = submessageToEntry(data);
      if (entry) {
        setEntries((prev) => [...prev, entry]);
      }
    });
    // Note: on_submessage doesn't return an unsubscribe function
    // This is a limitation we accept for now - callbacks persist for widget lifetime
  }, [ctx]);

  // Auto-scroll to bottom when entries change
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries.length, autoScroll]);

  const handleSubmit = async () => {
    if (!inputText.trim() || sending) return;

    setSending(true);
    try {
      if (ctx.post_submessage) {
        // Use submessage API for live updates
        await ctx.post_submessage({
          type: 'transcript_entry',
          speaker: ctx.current_user?.full_name ?? 'Anonymous',
          text: inputText.trim(),
          timestamp: Math.floor(Date.now() / 1000),
          avatarUrl: ctx.current_user?.avatar_url,
        });
      } else {
        // Fallback to interaction (for testing without submessage support)
        ctx.post_interaction({
          type: 'transcript_entry',
          speaker: ctx.current_user?.full_name ?? 'Anonymous',
          text: inputText.trim(),
        });
      }
      setInputText('');
    } finally {
      setSending(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <Card className="tulip-widget-transcript tw-overflow-hidden">
      {title && (
        <CardHeader className="tw-pb-2">
          <CardTitle className="tw-text-base">{title}</CardTitle>
        </CardHeader>
      )}
      <CardContent className={cn('tw-p-4', title && 'tw-pt-0')}>
        {/* Scrollable entries list */}
        <div
          ref={scrollRef}
          className="tw-overflow-y-auto tw-divide-y tw-divide-border"
          style={{ maxHeight: `${maxHeight}px` }}
        >
          {entries.length === 0 ? (
            <div className="tw-py-8 tw-text-center tw-text-muted-foreground tw-text-sm">
              No entries yet. Be the first to contribute!
            </div>
          ) : (
            entries.map((entry, i) => (
              <TranscriptEntryRow
                key={`${entry.speaker}-${entry.timestamp ?? i}`}
                entry={entry}
                showTimestamp={showTimestamps}
              />
            ))
          )}
        </div>

        {/* Input field */}
        {inputEnabled && (
          <div className="tw-mt-4 tw-flex tw-gap-2">
            <input
              type="text"
              value={inputText}
              onChange={(e) => setInputText(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={inputPlaceholder}
              disabled={sending}
              className={cn(
                'tw-flex-1 tw-px-3 tw-py-2 tw-text-sm',
                'tw-border tw-border-input tw-rounded-md',
                'tw-bg-background tw-text-foreground',
                'placeholder:tw-text-muted-foreground',
                'focus:tw-outline-none focus:tw-ring-2 focus:tw-ring-ring',
                'disabled:tw-opacity-50'
              )}
            />
            <Button
              variant="default"
              size="sm"
              onClick={handleSubmit}
              disabled={!inputText.trim() || sending}
              className="tw-px-3"
            >
              {inputButtonText === 'Send' ? (
                <Send className="tw-h-4 tw-w-4" />
              ) : (
                inputButtonText
              )}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
