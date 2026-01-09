import { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Portrait } from './Portrait';
import { useInteraction } from '@/context';
import { cn } from '@/lib/utils';
import type { DialogueProps } from '@/types';
import { ChevronDown, ChevronUp } from 'lucide-react';

export function Dialogue({
  speaker,
  text,
  choices,
  continueCustomId,
  history,
}: DialogueProps) {
  const { postInteraction } = useInteraction();
  const [historyExpanded, setHistoryExpanded] = useState(false);
  const [pendingChoice, setPendingChoice] = useState<string | null>(null);

  const handleChoice = (customId: string) => {
    setPendingChoice(customId);
    postInteraction({
      type: 'choice',
      customId,
    });
  };

  const handleContinue = () => {
    if (continueCustomId) {
      setPendingChoice(continueCustomId);
      postInteraction({
        type: 'continue',
        customId: continueCustomId,
      });
    }
  };

  return (
    <div className="tulip-widget-dialogue">
      {/* History section */}
      {history && history.length > 0 && (
        <div className="tw-mb-2">
          <button
            onClick={() => setHistoryExpanded(!historyExpanded)}
            className="tw-flex tw-items-center tw-gap-1 tw-text-xs tw-text-muted-foreground hover:tw-text-foreground tw-transition-colors"
          >
            {historyExpanded ? (
              <ChevronUp className="tw-h-3 tw-w-3" />
            ) : (
              <ChevronDown className="tw-h-3 tw-w-3" />
            )}
            {historyExpanded ? 'Hide' : 'Show'} previous ({history.length})
          </button>

          {historyExpanded && (
            <div className="tw-mt-2 tw-space-y-2 tw-pl-2 tw-border-l-2 tw-border-muted">
              {history.map((entry, i) => (
                <div key={i} className="tw-text-sm tw-text-muted-foreground">
                  {entry.speaker && (
                    <span className="tw-font-medium">{entry.speaker}: </span>
                  )}
                  <span className="tw-italic">{entry.text}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Main dialogue card */}
      <Card className="tw-overflow-hidden">
        <CardContent className="tw-p-4">
          <div className="tw-flex tw-gap-4">
            {/* Portrait */}
            {speaker && (
              <div className="tw-flex-shrink-0">
                <Portrait
                  name={speaker.name}
                  imageUrl={speaker.portraitUrl}
                  mood={speaker.mood}
                  size="lg"
                />
              </div>
            )}

            {/* Text content */}
            <div className="tw-flex-1 tw-min-w-0">
              {speaker && (
                <div className="tw-font-semibold tw-text-sm tw-mb-1">
                  {speaker.name}
                </div>
              )}
              <div className="tw-text-foreground tw-leading-relaxed">
                {text}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Choices */}
      {choices && choices.length > 0 && (
        <div className="tw-flex tw-flex-col tw-gap-2">
          {choices.map((choice) => (
            <Button
              key={choice.customId}
              variant="choice"
              size="choice"
              disabled={choice.disabled || pendingChoice !== null}
              onClick={() => handleChoice(choice.customId)}
              className={cn(
                'tw-w-full',
                pendingChoice === choice.customId && 'tw-opacity-70'
              )}
            >
              <div className="tw-flex tw-flex-col tw-items-start tw-gap-0.5">
                <span>{choice.label}</span>
                {choice.requirementText && (
                  <span className="tw-text-xs tw-text-muted-foreground">
                    {choice.requirementText}
                  </span>
                )}
              </div>
            </Button>
          ))}
        </div>
      )}

      {/* Continue button */}
      {continueCustomId && !choices?.length && (
        <Button
          variant="action"
          onClick={handleContinue}
          disabled={pendingChoice !== null}
          className="tw-w-full"
        >
          Continue
        </Button>
      )}
    </div>
  );
}
