import { AlertTriangle, CheckCircle2 } from 'lucide-react';

interface ToastProps {
  type: 'ok' | 'err';
  text: string;
}

export function Toast({ type, text }: ToastProps) {
  return (
    <div className={`mw-toast mw-toast-${type}`} role={type === 'err' ? 'alert' : 'status'}>
      {type === 'ok'
        ? <CheckCircle2 className="w-4 h-4 shrink-0" aria-hidden="true" />
        : <AlertTriangle className="w-4 h-4 shrink-0" aria-hidden="true" />}
      <span>{text}</span>
    </div>
  );
}
