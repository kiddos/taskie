export interface Event {
  id?: number;
  start?: string | null;
  end?: string | null;
  title?: string;
  description?: string;
  color?: string;
  notified?: string | null;
}
