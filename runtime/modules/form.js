// runtime/modules/form.js — Form validation runtime

const FormRuntime = {
  _forms: new Map(),
  _errors: new Map(),

  registerForm(namePtr, nameLen, schemaPtr, schemaLen) {
    // Note: requires core runtime's readString to be passed or available globally
    const name = typeof namePtr === 'string' ? namePtr : '';
    const schema = typeof schemaPtr === 'string' ? JSON.parse(schemaPtr) : {};
    FormRuntime._forms.set(name, { schema, values: {}, errors: {}, dirty: {}, touched: {} });
  },

  validate(namePtr, nameLen) {
    const name = typeof namePtr === 'string' ? namePtr : '';
    const form = FormRuntime._forms.get(name);
    if (!form) return 0;
    let valid = true;
    form.errors = {};
    for (const field of form.schema.fields) {
      const value = form.values[field.name];
      for (const v of field.validators) {
        const error = FormRuntime._runValidator(v, value, field.name);
        if (error) { form.errors[field.name] = error; valid = false; break; }
      }
    }
    return valid ? 1 : 0;
  },

  setFieldError(namePtr, nameLen, errorPtr, errorLen) {
    const name = typeof namePtr === 'string' ? namePtr : '';
    const error = typeof errorPtr === 'string' ? errorPtr : '';
    const form = FormRuntime._forms.get(name);
    if (form) form.errors[name] = error;
  },

  _runValidator(validator, value, fieldName) {
    switch (validator.kind) {
      case 'required': return (!value || value === '') ? (validator.message || `${fieldName} is required`) : null;
      case 'min_length': return (value && value.length < validator.min) ? (validator.message || `${fieldName} must be at least ${validator.min} characters`) : null;
      case 'max_length': return (value && value.length > validator.max) ? (validator.message || `${fieldName} must be at most ${validator.max} characters`) : null;
      case 'pattern': return (value && !new RegExp(validator.pattern).test(value)) ? (validator.message || `${fieldName} format is invalid`) : null;
      case 'email': return (value && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) ? (validator.message || `${fieldName} must be a valid email`) : null;
      case 'url': { try { new URL(value); return null; } catch { return validator.message || `${fieldName} must be a valid URL`; } }
      default: return null;
    }
  },

  getErrors(name) { return FormRuntime._forms.get(name)?.errors || {}; },
  isDirty(name) { const f = FormRuntime._forms.get(name); return f ? Object.keys(f.dirty).length > 0 : false; },
  reset(name) { const f = FormRuntime._forms.get(name); if (f) { f.values = {}; f.errors = {}; f.dirty = {}; f.touched = {}; } },
};

const formModule = {
  name: 'form',
  runtime: FormRuntime,
  wasmImports: {
    form: {
      registerForm: FormRuntime.registerForm,
      validate: FormRuntime.validate,
      setFieldError: FormRuntime.setFieldError,
    }
  }
};

if (typeof module !== "undefined") module.exports = formModule;
