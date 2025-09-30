use std::{
    ffi::{CStr, CString, NulError, c_char, c_void},
    fs::read_dir,
    path::PathBuf,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TranslatorError {
    #[error("Failed to create C string: {0}")]
    StringConversion(#[from] NulError),

    #[error("Translation failed: {0}")]
    TranslationFailed(String),

    #[error("Failed to create translator")]
    TranslatorCreationFailed,
}

pub struct ModelFiles {
    src_vocab_path: PathBuf,
    trg_vocab_path: PathBuf,
    model_path: PathBuf,
    shortlist_path: PathBuf,
}

impl ModelFiles {
    pub fn new(
        src_vocab_path: PathBuf,
        trg_vocab_path: PathBuf,
        model_path: PathBuf,
        shortlist_path: PathBuf,
    ) -> Self {
        ModelFiles {
            src_vocab_path,
            trg_vocab_path,
            model_path,
            shortlist_path,
        }
    }

    pub fn to_config(self) -> String {
        format!(
            r#"
beam-size: 1
normalize: 1.0
word-penalty: 0
max-length-break: 128
mini-batch-words: 1024
workspace: 128
max-length-factor: 2.0
skip-cost: True
quiet: True
quiet_translation: True
gemm-precision: int8shiftAll

models: [{}]
vocabs: [{}, {}]
shortlist: [{}, false]
"#,
            self.model_path.display(),
            self.src_vocab_path.display(),
            self.trg_vocab_path.display(),
            self.shortlist_path.display()
        )
    }
}

impl From<&str> for ModelFiles {
    fn from(base_dir: &str) -> Self {
        ModelFiles::from(PathBuf::from(base_dir))
    }
}

impl From<PathBuf> for ModelFiles {
    fn from(base_dir: PathBuf) -> Self {
        let mut files = ModelFiles {
            src_vocab_path: PathBuf::new(),
            trg_vocab_path: PathBuf::new(),
            model_path: PathBuf::new(),
            shortlist_path: PathBuf::new(),
        };

        for entry in read_dir(base_dir).unwrap_or_else(|e| {
            panic!("Failed to read directory: {}", e);
        }) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    eprintln!("Failed to read entry: {}", e);
                    continue;
                }
            };
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();

            match file_name.as_ref() {
                name if name.ends_with(".spm") => {
                    if name.starts_with("srcvocab") {
                        files.src_vocab_path = path;
                    } else if name.starts_with("trgvocab") {
                        files.trg_vocab_path = path;
                    } else {
                        files.src_vocab_path = path.clone();
                        files.trg_vocab_path = path;
                    }
                }
                name if name.ends_with(".intgemm.alphas.bin")
                    || name.ends_with(".intgemm8.bin") =>
                {
                    files.model_path = path;
                }
                name if name.ends_with(".s2t.bin") => {
                    files.shortlist_path = path;
                }
                _ => {}
            }
        }
        files
    }
}

#[link(name = "linguaspark")]
unsafe extern "C" {
    fn bergamot_create(numWorkers: usize) -> *mut c_void;
    fn bergamot_destroy(translator: *mut c_void);
    fn bergamot_load_model_from_config(
        translator: *mut c_void,
        languagePair: *const c_char,
        config: *const c_char,
    );
    fn bergamot_is_supported(
        translator: *mut c_void,
        from: *const c_char,
        to: *const c_char,
    ) -> bool;
    fn bergamot_translate(
        translator: *mut c_void,
        from: *const c_char,
        to: *const c_char,
        input: *const c_char,
    ) -> *const c_char;
    fn bergamot_free_translation(translation: *const c_char);
}

pub struct Translator {
    inner: *mut c_void,
}

unsafe impl Send for Translator {}
unsafe impl Sync for Translator {}

impl Translator {
    pub fn new(num_workers: usize) -> Result<Self, TranslatorError> {
        let inner = unsafe { bergamot_create(num_workers) };
        if inner.is_null() {
            return Err(TranslatorError::TranslatorCreationFailed);
        }
        Ok(Translator { inner })
    }

    pub fn load_model<Model: Into<ModelFiles>>(
        &self,
        language_pair: &str,
        model: Model,
    ) -> Result<(), TranslatorError> {
        let language_pair_cstr = CString::new(language_pair)?;
        let config_cstr = CString::new(model.into().to_config())?;
        unsafe {
            bergamot_load_model_from_config(
                self.inner,
                language_pair_cstr.as_ptr(),
                config_cstr.as_ptr(),
            );
        }
        Ok(())
    }

    pub fn is_supported(&self, from_lang: &str, to_lang: &str) -> Result<bool, TranslatorError> {
        let from_cstr = CString::new(from_lang)?;
        let to_cstr = CString::new(to_lang)?;
        let supported =
            unsafe { bergamot_is_supported(self.inner, from_cstr.as_ptr(), to_cstr.as_ptr()) };
        Ok(supported)
    }

    pub fn translate(
        &self,
        from_lang: &str,
        to_lang: &str,
        input_text: &str,
    ) -> Result<String, TranslatorError> {
        let from_cstr = CString::new(from_lang)?;
        let to_cstr = CString::new(to_lang)?;
        let input_cstr = CString::new(input_text)?;
        let translated_text_ptr = unsafe {
            bergamot_translate(
                self.inner,
                from_cstr.as_ptr(),
                to_cstr.as_ptr(),
                input_cstr.as_ptr(),
            )
        };

        if translated_text_ptr.is_null() {
            // Workaround for null pointer error: try adding a "/" prefix
            let modified_input = format!("/{}", input_text);
            let modified_input_cstr = CString::new(modified_input)?;
            let retry_ptr = unsafe {
                bergamot_translate(
                    self.inner,
                    from_cstr.as_ptr(),
                    to_cstr.as_ptr(),
                    modified_input_cstr.as_ptr(),
                )
            };

            if retry_ptr.is_null() {
                return Err(TranslatorError::TranslationFailed(
                    "null pointer returned".to_string(),
                ));
            }

            let translated_text_cstr = unsafe { CStr::from_ptr(retry_ptr) };
            let mut translated_text = translated_text_cstr.to_string_lossy().into_owned();
            
            // Remove the leading "/" from the result if it exists
            if translated_text.starts_with('/') {
                translated_text = translated_text[1..].to_string();
            }

            unsafe { bergamot_free_translation(retry_ptr) };
            return Ok(translated_text);
        }

        let translated_text_cstr = unsafe { CStr::from_ptr(translated_text_ptr) };
        let translated_text = translated_text_cstr.to_string_lossy().into_owned();

        unsafe { bergamot_free_translation(translated_text_ptr) };

        Ok(translated_text)
    }
}

impl Drop for Translator {
    fn drop(&mut self) {
        unsafe {
            bergamot_destroy(self.inner);
        }
    }
}

#[doc(hidden)]
#[unsafe(no_mangle)]
pub extern "C" fn mkl_serv_intel_cpu_true() -> i32 {
    // https://documentation.sigma2.no/jobs/mkl.html
    // https://danieldk.eu/Intel-MKL-on-AMD-Zen
    1
}
