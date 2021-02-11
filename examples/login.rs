use druid::{
    lens,
    widget::{Button, Controller, Flex, Label, TextBox},
    AppLauncher, Data, Env, Event, EventCtx, Lens, PlatformError, Selector, Widget, WidgetExt,
    WindowDesc,
};
use druid_enums::Matcher;
use std::marker::PhantomData;

const LOGIN: Selector<MainState> = Selector::new("druid-enums.basic.login");

/// A lens that applies two lenses to the same data
/// Almost like a parallel version of Then
///
/// This lets us get both the shared state and the target enum
/// out of the larger state
pub struct ChainLens<In, Lens1, Lens2> {
    lens1: Lens1,
    lens2: Lens2,
    phantom_in: PhantomData<In>,
}

impl<In, Lens1, Lens2> ChainLens<In, Lens1, Lens2> {
    pub fn new(lens1: Lens1, lens2: Lens2) -> ChainLens<In, Lens1, Lens2> {
        ChainLens {
            lens1,
            lens2,
            phantom_in: PhantomData,
        }
    }
}

impl<In, Lens1, Lens2, Out1, Out2> Lens<In, (Out1, Out2)> for ChainLens<In, Lens1, Lens2>
where
    In: Data,
    Lens1: Lens<In, Out1>,
    Lens2: Lens<In, Out2>,
    Out1: Data,
    Out2: Data,
{
    fn with<R, F: FnOnce(&(Out1, Out2)) -> R>(&self, data: &In, f: F) -> R {
        self.lens1.with(data, |out1| {
            self.lens2
                .with(data, |out2| f(&(out1.to_owned(), out2.to_owned())))
        })
    }

    fn with_mut<R, F: FnOnce(&mut (Out1, Out2)) -> R>(&self, data: &mut In, f: F) -> R {
        // Because we cannot apply both lenses mutably to the data twice at the same time,
        // we make two data's, one for each lens
        let mut data1 = data.to_owned();
        let mut data2 = data.to_owned();

        // Now we can apply to two lenses to each of the two data's at the same time
        let result = self.lens1.with_mut(&mut data1, |out1| {
            self.lens2.with_mut(&mut data2, |out2| {
                let mut out = (out1.to_owned(), out2.to_owned());

                let result = f(&mut out);

                if !out.0.same(out1) {
                    *out1 = out.0;
                }

                if !out.1.same(out2) {
                    *out2 = out.1;
                }

                result
            })
        });

        // Now we can go back and take what changed out of the two data's and put them into
        // the original data
        let new_out1 = self.lens1.with(&data1, |out1| out1.to_owned());
        self.lens1.with_mut(data, |out1| {
            if !out1.same(&new_out1) {
                *out1 = new_out1;
            }
        });

        let new_out2 = self.lens2.with(&data2, |out2| out2.to_owned());
        self.lens2.with_mut(data, |out2| {
            if !out2.same(&new_out2) {
                *out2 = new_out2;
            }
        });

        result
    }
}

#[derive(Clone, Data, Lens, Debug)]
struct State {
    app: AppState,
    string: String,
}

#[derive(Clone, Data, Matcher, Debug)]
#[matcher(matcher_name = App)] // defaults to AppStateMatcher
enum AppState {
    Login(LoginState),
    Main(MainState),
}

#[derive(Clone, Data, Lens, Default, Debug)]
struct LoginState {
    user: String,
}

#[derive(Clone, Data, Lens, Debug)]
struct MainState {
    user: String,
    count: u32,
}

fn main() -> Result<(), PlatformError> {
    let window = WindowDesc::new(ui).title("Druid Enums");
    let state = State {
        app: AppState::Login(LoginState::default()),
        string: "Hello".to_string(),
    };
    AppLauncher::with_window(window)
        .use_simple_logger()
        .launch(state)
}

fn ui() -> impl Widget<State> {
    Flex::column()
        .with_child(
            // AppState::matcher() or
            App::new()
                .login(login_ui())
                .main(main_ui())
                .controller(LoginController)
                .lens(ChainLens::new(State::string, State::app)),
        )
        .with_child(TextBox::new().lens(State::string))
}

fn login_ui() -> impl Widget<(String, LoginState)> {
    fn login(ctx: &mut EventCtx, (_string, login_state): &mut (String, LoginState), _: &Env) {
        ctx.submit_command(LOGIN.with(MainState::from(login_state.clone())), None)
    }

    Flex::row()
        .with_child(
            TextBox::new()
                .lens(LoginState::user)
                .lens(lens!((String, LoginState), 1)),
        )
        .with_spacer(5.0)
        .with_child(Button::new("Login").on_click(login))
        .center()
}

fn main_ui() -> impl Widget<(String, MainState)> {
    Flex::column()
        .with_child(Label::dynamic(MainState::welcome_label).lens(lens!((String, MainState), 1)))
        .with_spacer(5.0)
        .with_child(
            Button::dynamic(MainState::count_label)
                .on_click(|_, state: &mut MainState, _| state.count += 1)
                .lens(lens!((String, MainState), 1)),
        )
        .with_child(Label::new(
            |(string, main_state): &(String, MainState), _env: &Env| {
                string
                    .chars()
                    .nth(main_state.count as usize)
                    .map(|c| format!("{}", c))
                    .unwrap_or("Out of bounds!".to_string())
            },
        ))
        .center()
}

struct LoginController;
impl Controller<(String, AppState), App<String>> for LoginController {
    fn event(
        &mut self,
        child: &mut App<String>,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut (String, AppState),
        env: &Env,
    ) {
        match event {
            Event::Command(cmd) if cmd.is(LOGIN) => {
                let main_state = cmd.get_unchecked(LOGIN).clone();
                data.1 = AppState::Main(main_state);
            }
            _ => {}
        }
        child.event(ctx, event, data, env)
    }
}

impl MainState {
    pub fn welcome_label(&self, _: &Env) -> String {
        format!("Welcome {}!", self.user)
    }

    pub fn count_label(&self, _: &Env) -> String {
        format!("clicked {} times", self.count)
    }
}

impl From<LoginState> for MainState {
    fn from(login: LoginState) -> Self {
        MainState {
            user: login.user,
            count: 0,
        }
    }
}
