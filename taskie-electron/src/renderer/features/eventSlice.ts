import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { Event } from '../../types/event';
import { AppDispatch } from '../app/store';
import { IPC_CHANNELS } from '../../types/ipc_channels';

interface EventState {
  events: Event[];
  loading: boolean;
  error: string | null;
}

const initialState: EventState = {
  events: [],
  loading: false,
  error: null,
};

export const eventSlice = createSlice({
  name: 'event',
  initialState,
  reducers: {
    setLoading(state, action: PayloadAction<boolean>) {
      state.loading = action.payload;
    },
    setError(state, action: PayloadAction<string>) {
      state.error = action.payload;
    },
    createEvent(state, action: PayloadAction<Event>) {
      state.events.push(action.payload);
    },
    updateEvent(state, action: PayloadAction<Event>) {
      const index = state.events.findIndex(
        (event) => event.id === action.payload.id
      );
      if (index !== -1) {
        state.events[index] = action.payload;
      }
    },
    deleteEvent(state, action: PayloadAction<number>) {
      state.events = state.events.filter(
        (event) => event.id !== action.payload
      );
    },
    setEvents(state, action: PayloadAction<Event[]>) {
      state.events = action.payload;
    },
  },
});

// Action creators are generated for each case reducer function
export const {
  setLoading,
  setError,
  createEvent,
  updateEvent,
  deleteEvent,
  setEvents,
} = eventSlice.actions;

// Async thunks for CRUD operations
export const createEventAsync = (event: Event) => (dispatch: AppDispatch) => {
  dispatch(setLoading(true));
  try {
    window.electron.ipcRenderer.sendMessage(IPC_CHANNELS.CREATE_EVENT, event);
    window.electron.ipcRenderer.once(
      IPC_CHANNELS.CREATE_EVENT,
      (response: any) => {
        const eventId = response as number;
        event.id = eventId;
        dispatch(createEvent(event));
      }
    );
  } catch (error: any) {
    dispatch(setError(error.message));
  } finally {
    dispatch(setLoading(false));
  }
};

export const updateEventAsync = (event: Event) => (dispatch: AppDispatch) => {
  dispatch(setLoading(true));
  try {
    window.electron.ipcRenderer.sendMessage(IPC_CHANNELS.UPDATE_EVENT, event);
    window.electron.ipcRenderer.once(
      IPC_CHANNELS.UPDATE_EVENT,
      (response: any) => {
        const result = response as boolean;
        if (result) {
          dispatch(updateEvent(event));
        }
      }
    );
  } catch (error: any) {
    dispatch(setError(error.message));
  } finally {
    dispatch(setLoading(false));
  }
};

export const deleteEventAsync = (id: number) => (dispatch: AppDispatch) => {
  dispatch(setLoading(true));
  try {
    window.electron.ipcRenderer.sendMessage(IPC_CHANNELS.DELETE_EVENT, id);
    window.electron.ipcRenderer.once(
      IPC_CHANNELS.DELETE_EVENT,
      (response: any) => {
        const result = response as boolean;
        if (result) {
          dispatch(deleteEvent(id));
        }
      }
    );
  } catch (error: any) {
    dispatch(setError(error.message));
  } finally {
    dispatch(setLoading(false));
  }
};

export const getEventsAsync = () => async (dispatch: AppDispatch) => {
  dispatch(setLoading(true));
  try {
    window.electron.ipcRenderer.sendMessage(IPC_CHANNELS.GET_EVENTS);
    window.electron.ipcRenderer.once(
      IPC_CHANNELS.GET_EVENTS,
      (response: any) => {
        dispatch(setEvents(response as Event[]));
      }
    );
  } catch (error: any) {
    dispatch(setError(error.message));
  } finally {
    dispatch(setLoading(false));
  }
};

export default eventSlice.reducer;
