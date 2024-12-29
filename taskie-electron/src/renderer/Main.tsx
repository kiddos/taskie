import { useEffect, useRef } from 'react';
import { Calendar, globalizeLocalizer } from 'react-big-calendar';
import { useDispatch, useSelector } from 'react-redux';
import globalize from 'globalize';
import { getEventsAsync } from './features/eventSlice';
import { RootState, AppDispatch } from './app/store';
import { Event } from '../types/event';
import EventForm from './EventForm';

const localizer = globalizeLocalizer(globalize);

function CalendarView() {
  const dispatch = useDispatch<AppDispatch>();
  const form = useRef<any>(null);
  const { events } = useSelector((state: RootState) => state.events);

  useEffect(() => {
    dispatch(getEventsAsync());
  }, [dispatch]);

  const handleSelectEvent = (event: any) => {
    if (form.current) {
      form.current.open({
        id: event.id,
        title: event.title || '',
        description: event.description || '',
        start: event.start?.toISOString(),
        end: event.end?.toISOString(),
        color: event.color,
      });
    }
  };

  const handleSelectSlot = (event: any) => {
    if (form.current) {
      form.current.open({
        title: '',
        description: '',
        start: event.start?.toISOString(),
        end: event.end?.toISOString(),
        color: 'blue',
      });
    }
  };

  const list = events.map((e) => ({
    ...e,
    start: e.start ? new Date(e.start) : null,
    end: e.end ? new Date(e.end) : null,
  }));

  const eventPropGetter = (event: Event) => {
    return {
      className: event.color,
    };
  };

  return (
    <div style={{ padding: 8 }}>
      <Calendar
        localizer={localizer}
        events={list}
        startAccessor="start"
        endAccessor="end"
        style={{ height: 500 }}
        onSelectEvent={handleSelectEvent}
        onSelectSlot={handleSelectSlot}
        selectable
        eventPropGetter={eventPropGetter}
      />
      <EventForm ref={form} />
    </div>
  );
}

function Main() {
  return (
    <div>
      <CalendarView />
    </div>
  );
}

export default Main;
