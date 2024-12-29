import { configureStore } from '@reduxjs/toolkit';
import eventReducer from '../features/eventSlice';

const store = configureStore({
  reducer: {
    events: eventReducer,
  },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;

export default store;
