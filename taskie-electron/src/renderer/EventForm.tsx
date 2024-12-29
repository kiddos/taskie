import { PureComponent } from 'react';
import {
  Modal,
  Form,
  ButtonToolbar,
  DatePicker,
  Button,
  RadioGroup,
  Radio,
  Schema,
} from 'rsuite';
import { connect } from 'react-redux';
import { Event } from '../types/event';
import {
  createEventAsync,
  updateEventAsync,
  deleteEventAsync,
} from './features/eventSlice';
import { AppDispatch } from './app/store';

const { StringType, DateType } = Schema.Types;

const model = Schema.Model({
  title: StringType().isRequired('Title is required.'),
  start: DateType().isRequired('Start Date is required'),
  end: DateType().isRequired('End Date is required'),
});

interface DateFieldProps {
  name: string;
  value: string | null;
  onChange: (value: string | null) => void;
  [key: string]: any;
}

function DateInput({ name, value, onChange, ...props }: DateFieldProps) {
  return (
    <DatePicker
      name={name}
      value={value ? new Date(value) : undefined}
      onChange={(date: Date | null) =>
        onChange(date ? date?.toISOString() : null)
      }
      {...props}
      format="yyyy-MM-dd HH:mm"
    />
  );
}

const defaultEvent: Event = {
  title: '',
  start: null,
  end: null,
  description: '',
};

interface EventFormProps {
  dispatch: AppDispatch;
}

interface EventFormState {
  formValue: Event;
  open: boolean;
}

const colors = [
  { value: 'red', hex: '#f44336' },
  { value: 'green', hex: '#4caf50' },
  { value: 'blue', hex: '#265985' },
  { value: 'orange', hex: '#fa8900' },
  { value: 'purple', hex: '#673ab7' },
  { value: 'yellow', hex: '#ffb300' },
];

class EventForm extends PureComponent<EventFormProps, EventFormState> {
  constructor(props: EventFormProps) {
    super(props);

    this.state = {
      formValue: { ...defaultEvent },
      open: false,
    };
  }

  handleChange = (value: any) => {
    if (!value.end && value.start) {
      const d = new Date(value.start);
      d.setHours(d.getHours() + 1);
      value.end = d.toISOString();
    }
    this.setState({ formValue: value });
  };

  handleSubmit = async () => {
    const { formValue } = this.state;
    const { dispatch } = this.props;

    if (formValue.id) {
      dispatch(updateEventAsync(formValue));
    } else {
      dispatch(createEventAsync(formValue));
    }
    this.handleClose();
  };

  handleClose = () => {
    this.setState({ open: false });
  };

  handleDelete = () => {
    const { formValue } = this.state;
    const { dispatch } = this.props;
    if (formValue.id) {
      dispatch(deleteEventAsync(formValue.id));
      this.handleClose();
    }
  };

  handleColorChange = (value: any) => {
    this.setState((prevState) => ({
      ...prevState,
      formValue: {
        ...prevState.formValue,
        color: value as string,
      },
    }));
  };

  open = (newEvent: Event) => {
    this.setState({
      open: true,
      formValue: {
        ...newEvent,
      },
    });
  };

  render() {
    const { open, formValue } = this.state;
    const title = formValue.id ? 'Update Event' : 'Create Event';
    return (
      <Modal backdrop="static" open={open}>
        <div style={{ width: 400, margin: '0 auto', padding: '20px' }}>
          <h3>{title}</h3>
          <Form
            model={model}
            formValue={formValue}
            onChange={this.handleChange}
            onSubmit={this.handleSubmit}
            fluid
          >
            <Form.Group>
              <Form.ControlLabel>Event Title</Form.ControlLabel>
              <Form.Control name="title" />
            </Form.Group>

            <Form.Group>
              <Form.ControlLabel>Start Date</Form.ControlLabel>
              <Form.Control name="start" accepter={DateInput} />
            </Form.Group>

            <Form.Group>
              <Form.ControlLabel>End Date</Form.ControlLabel>
              <Form.Control name="end" accepter={DateInput} />
            </Form.Group>

            <Form.Group>
              <Form.ControlLabel>Description</Form.ControlLabel>
              <Form.Control name="description" />
            </Form.Group>

            <RadioGroup
              name="color"
              onChange={this.handleColorChange}
              value={formValue.color}
              inline
            >
              {colors.map((color) => (
                <Radio key={color.value} value={color.value}>
                  <div
                    style={{
                      backgroundColor: color.hex,
                      width: 20,
                      height: 20,
                      borderRadius: 10,
                      margin: 5,
                      transform: 'translate(0, -6px)',
                    }}
                  />
                </Radio>
              ))}
            </RadioGroup>

            <ButtonToolbar>
              <Button
                appearance="primary"
                style={{ width: '100%' }}
                type="submit"
              >
                Submit
              </Button>

              {formValue.id ? (
                <Button
                  color="red"
                  appearance="primary"
                  style={{ width: '100%' }}
                  onClick={this.handleDelete}
                >
                  Delete
                </Button>
              ) : null}

              <Button style={{ width: '100%' }} onClick={this.handleClose}>
                Cancel
              </Button>
            </ButtonToolbar>
          </Form>
        </div>
      </Modal>
    );
  }
}

export default connect(null, null, null, { forwardRef: true })(EventForm);
