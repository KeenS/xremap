use crate::client::Client;
use std::env;
use std::ffi::CString;
use std::ptr::NonNull;
use x11_rs::xlib;

pub struct X11Client {
    display: Option<NonNull<xlib::Display>>,
}

impl X11Client {
    pub fn new() -> X11Client {
        X11Client { display: None }
    }

    fn connect(&mut self) {
        match self.display {
            Some(_) => (),
            None => {
                if let Err(env::VarError::NotPresent) = env::var("DISPLAY") {
                    println!("$DISPLAY is not set. Defaulting to DISPLAY=:0");
                    env::set_var("DISPLAY", ":0");
                }

                let display = unsafe { xlib::XOpenDisplay(std::ptr::null()) };
                self.display = NonNull::new(display);
                if self.display.is_none() {
                    let var = env::var("DISPLAY").unwrap();
                    println!("warning: Failed to connect to X11.");
                    println!("If you saw \"No protocol specified\", try running `xhost +SI:localuser:root`.");
                    println!("If not, make sure `echo $DISPLAY` outputs xremap's $DISPLAY ({}).", var);
                };
            }
        }
    }

    fn as_mut_ptr(&mut self) -> *mut xlib::Display {
        match self.display {
            Some(d) => d.as_ptr(),
            None => std::ptr::null_mut(),
        }
    }

    fn get_input_focus_window(&mut self) -> xlib::Window {
        let mut focused_window = 0;
        let mut focus_state = 0;
        unsafe { xlib::XGetInputFocus(self.as_mut_ptr(), &mut focused_window, &mut focus_state) };
        return focused_window;
    }

    fn get_class_hint_class(&mut self, window: xlib::Window) -> Option<String> {
        let mut x_class_hint = xlib::XClassHint {
            res_name: std::ptr::null_mut(),
            res_class: std::ptr::null_mut(),
        };

        unsafe {
            if xlib::XGetClassHint(self.as_mut_ptr(), window, &mut x_class_hint) == 1 {
                if !x_class_hint.res_name.is_null() {
                    xlib::XFree(x_class_hint.res_name as *mut std::ffi::c_void);
                }

                if !x_class_hint.res_class.is_null() {
                    // Note: into_string() seems to free `x_class_hint.res_class`. So XFree isn't needed.
                    let wm_class = CString::from_raw(x_class_hint.res_class as *mut i8)
                        .into_string()
                        .unwrap();
                    Some(wm_class)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    fn query_tree_parent(&mut self, window: xlib::Window) -> Option<xlib::Window> {
        let mut nchildren: u32 = 0;
        let mut root: xlib::Window = 0;
        let mut children: *mut xlib::Window = &mut 0;
        let mut parent: xlib::Window = 0;
        unsafe {
            if xlib::XQueryTree(self.as_mut_ptr(), window, &mut root, &mut parent, &mut children, &mut nchildren) == 0 {
                None
            } else {
                if !children.is_null() {
                    xlib::XFree(children as *mut std::ffi::c_void);
                }
                Some(parent)
            }
        }
    }
}

impl Client for X11Client {
    fn supported(&mut self) -> bool {
        self.connect();
        if self.display.is_none() {
            false
        } else {
            let focused_window = self.get_input_focus_window();
            focused_window > 0
        }
    }

    fn current_application(&mut self) -> Option<String> {
        if !self.supported() {
            return None;
        }

        self.connect();

        let mut focused_window = self.get_input_focus_window();

        let mut wm_class = String::new();
        loop {
            if let Some(class) = self.get_class_hint_class(focused_window) {
                wm_class = class;
            }
            // Workaround: https://github.com/JetBrains/jdk8u_jdk/blob/master/src/solaris/classes/sun/awt/X11/XFocusProxyWindow.java#L35
            if &wm_class != "FocusProxy" {
                break;
            }
            let parent = match self.query_tree_parent(focused_window) {
                // The root client's parent is NULL. Avoid querying it to prevent SEGV on XGetClientHint.
                Some(parent) if parent == 0 => return None,
                Some(parent) => parent,
                None => break,
            };
            focused_window = parent;
        }
        Some(wm_class)
    }
}
