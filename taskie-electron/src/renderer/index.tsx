import { createRoot } from 'react-dom/client';
import App from './App';
import 'rsuite/dist/rsuite.min.css';
import 'react-big-calendar/lib/css/react-big-calendar.css';

const container = document.getElementById('root') as HTMLElement;
const root = createRoot(container);
root.render(<App />);
