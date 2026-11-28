import icon from '@/app/icon.png';
import { appName } from '@/lib/shared';

export function Brand() {
  return (
    <span className="inline-flex items-center gap-2 text-[15px] font-semibold text-heka-bright">
      <img src={icon.src} alt="" width={20} height={20} />
      {appName}
    </span>
  );
}
