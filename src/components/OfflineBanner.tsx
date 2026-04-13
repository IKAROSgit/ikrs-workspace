import { useOnlineStatus } from "@/hooks/useOnlineStatus";

interface OfflineBannerProps {
  feature: string;
}

export function OfflineBanner({ feature }: OfflineBannerProps) {
  const isOnline = useOnlineStatus();

  if (isOnline) return null;

  return (
    <div className="flex items-center gap-2 px-4 py-2 bg-amber-500/10 text-amber-700 dark:text-amber-400 text-sm border-b border-amber-500/20">
      <span>You're offline. {feature} requires an internet connection.</span>
    </div>
  );
}
