import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";
import type { ChatMessage } from "@/types/claude";

interface MessageBubbleProps {
  message: ChatMessage;
}

// react-markdown's custom components — declared at module scope so
// they're stable across re-renders (otherwise react-markdown treats
// each render as a config change and rebuilds its renderer tree).
const markdownComponents = {
  // Tauri's CSP blocks unknown-origin images; force lazy load.
  // biome-ignore lint/a11y/useAltText: alt text may come from markdown
  img: (props: React.ImgHTMLAttributes<HTMLImageElement>) => (
    <img loading="lazy" {...props} />
  ),
  // Inline code and fenced blocks: monospace + muted bg.
  code: ({
    className,
    children,
    ...props
  }: React.HTMLAttributes<HTMLElement> & {
    className?: string;
    children?: React.ReactNode;
  }) => {
    const isBlock = className?.includes("language-");
    if (isBlock) {
      return (
        <pre className="bg-background/60 border border-border rounded-md p-3 my-2 overflow-x-auto text-xs">
          <code className={className} {...props}>
            {children}
          </code>
        </pre>
      );
    }
    return (
      <code
        className="bg-background/60 rounded px-1 py-0.5 text-[0.9em]"
        {...props}
      >
        {children}
      </code>
    );
  },
  // Tables wrapped for mobile scroll.
  table: ({ children, ...props }: React.HTMLAttributes<HTMLTableElement>) => (
    <div className="overflow-x-auto my-2">
      <table className="w-full border-collapse" {...props}>
        {children}
      </table>
    </div>
  ),
  th: ({ children, ...props }: React.HTMLAttributes<HTMLTableCellElement>) => (
    <th
      className="border border-border px-2 py-1 text-left font-semibold bg-background/50"
      {...props}
    >
      {children}
    </th>
  ),
  td: ({ children, ...props }: React.HTMLAttributes<HTMLTableCellElement>) => (
    <td className="border border-border px-2 py-1" {...props}>
      {children}
    </td>
  ),
  // Safer external link target. Spread first, overrides last so
  // injected `target="_self"` from remote content can't win.
  a: ({
    children,
    ...props
  }: React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
    <a {...props} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  ),
} as const;

const remarkPlugins = [remarkGfm];

function MessageBubbleInner({ message }: MessageBubbleProps) {
  const isUser = message.role === "user";

  return (
    <div className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "rounded-lg px-4 py-2 text-sm max-w-[80%]",
          isUser
            ? "bg-primary text-primary-foreground whitespace-pre-wrap"
            : "bg-muted text-foreground",
        )}
      >
        {isUser ? (
          message.text
        ) : (
          // Inner prose wrapper so the bubble's `max-w-[80%]` still
          // caps width (putting `max-w-none` on the outer div would
          // defeat the cap for long markdown responses).
          <div className="prose prose-sm dark:prose-invert max-w-none">
            <ReactMarkdown
              remarkPlugins={remarkPlugins}
              components={markdownComponents}
            >
              {message.text}
            </ReactMarkdown>
          </div>
        )}
        {message.isStreaming && (
          <span className="inline-block w-1.5 h-4 ml-0.5 bg-current animate-pulse" />
        )}
      </div>
    </div>
  );
}

// Memoize — during a streaming turn we re-render on every token
// delta. React.memo cuts ~80% of the react-markdown work when only
// the tail delta changed but surrounding messages did not.
export const MessageBubble = memo(
  MessageBubbleInner,
  (prev, next) =>
    prev.message.id === next.message.id &&
    prev.message.text === next.message.text &&
    prev.message.isStreaming === next.message.isStreaming,
);
