#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, BlockInfo, Deps, DepsMut, Env, MessageInfo, Order, StdResult,
};

use cw0::{maybe_addr, Expiration};
use cw2::set_contract_version;
use cw3::{
    Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse,
    VoterResponse,
};
use cw_storage_plus::Bound;
use tg4::{MemberChangedHookMsg, MemberDiff, Tg4Contract};
use tg_bindings::TgradeMsg;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, OversightProposal, QueryMsg};
use crate::state::{
    next_id, parse_id, Ballot, Config, Proposal, ProposalListResponse, ProposalResponse, Votes,
    VotingRules, BALLOTS, CONFIG, PROPOSALS,
};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_oc_proposals";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let engagement_contract = Tg4Contract(
        deps.api
            .addr_validate(&msg.engagement_contract)
            .map_err(|_| ContractError::InvalidEngagementContract {
                addr: msg.engagement_contract.clone(),
            })?,
    );
    let total_weight = engagement_contract.total_weight(&deps.querier)?;
    msg.threshold.validate(total_weight)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cfg = Config {
        rules: msg.rules,
        group_addr,
        engagement_contract,
    };

    cfg.rules.validate()?;
    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
            latest,
        } => execute_propose(deps, env, info, title, description, proposal, latest),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::MemberChangedHook(MemberChangedHookMsg { diffs }) => {
            execute_membership_hook(deps, env, info, diffs)
        }
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: OversightProposal,
    // we ignore earliest
    latest: Option<Expiration>,
) -> Result<Response, ContractError> {
    // only members of the multisig can create a proposal
    let cfg = CONFIG.load(deps.storage)?;

    let vote_power = cfg
        .engagement_contract
        .is_member(&deps.querier, &info.sender)?
        .ok_or(ContractError::Unauthorized {})?;

    // calculate expiry time
    let expires = Expiration::AtTime(env.block.time.plus_seconds(cfg.rules.voting_period_secs()));

    // create a proposal
    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires,
        proposal,
        status: Status::Open,
        votes: Votes::new(vote_power),
        rules: cfg.rules,
        total_weight: cfg.group_addr.total_weight(&deps.querier)?,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    PROPOSALS.save(deps.storage, id.into(), &prop)?;

    // add the first yes vote from voter
    let ballot = Ballot {
        weight: vote_power,
        vote: Vote::Yes,
    };
    BALLOTS.save(deps.storage, (id.into(), &info.sender), &ballot)?;

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response, ContractError> {
    // only members of the multisig can vote
    let cfg = CONFIG.load(deps.storage)?;

    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;
    if prop.status != Status::Open {
        return Err(ContractError::NotOpen {});
    }
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // use a snapshot of "start of proposal"
    let vote_power = cfg
        .engagement_contract
        .member_at_height(&deps.querier, info.sender.clone(), prop.start_height)?
        .ok_or(ContractError::Unauthorized {})?;

    // cast vote if no vote previously cast
    BALLOTS.update(
        deps.storage,
        (proposal_id.into(), &info.sender),
        |bal| match bal {
            Some(_) => Err(ContractError::AlreadyVoted {}),
            None => Ok(Ballot {
                weight: vote_power,
                vote,
            }),
        },
    )?;

    // update vote tally
    prop.votes.add_vote(vote, vote_power);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id.into(), &prop)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    let engagement_contract = CONFIG.load(deps.storage)?.engagement_contract;

    let eng_admin = engagement_contract.admin(&deps.querier)?;
    if eng_admin.is_some() && eng_admin.unwrap() != env.contract.address {
        return Err(ContractError::ContractIsNotEngagementAdmin);
    }

    let message = match prop.proposal {
        OversightProposal::GrantEngagement { ref member, points } => {
            let member_weight = engagement_contract
                .member_at_height(&deps.querier, member.to_string(), env.block.height)?
                .ok_or(ContractError::EngagementMemberNotFound {
                    member: member.to_string(),
                })?;
            let member = tg4::Member {
                addr: member.to_string(),
                weight: member_weight + points,
            };
            engagement_contract.encode_raw_msg(to_binary(
                &tg4_engagement::ExecuteMsg::UpdateMembers {
                    remove: vec![],
                    add: vec![member],
                },
            )?)?
        }
    };

    // set it to executed
    prop.status = Status::Executed;
    PROPOSALS.save(deps.storage, proposal_id.into(), &prop)?;

    // dispatch all proposed messages
    Ok(Response::new()
        .add_submessage(message)
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;
    if [Status::Executed, Status::Rejected, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;
    PROPOSALS.save(deps.storage, proposal_id.into(), &prop)?;

    Ok(Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_membership_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _diffs: Vec<MemberDiff>,
) -> Result<Response, ContractError> {
    // This is now a no-op
    // But we leave the authorization check as a demo
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.engagement_contract.0 {
        return Err(ContractError::Unauthorized {});
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Rules {} => to_binary(&query_rules(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit)?)
        }
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, env, start_before, limit)?),
        QueryMsg::ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(deps, proposal_id, start_after, limit)?),
        QueryMsg::Voter { address } => to_binary(&query_voter(deps, address)?),
        QueryMsg::ListVoters { start_after, limit } => {
            to_binary(&list_voters(deps, start_after, limit)?)
        }
    }
}

fn query_rules(deps: Deps) -> StdResult<VotingRules> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.rules)
}

fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id.into())?;
    let status = prop.current_status(&env.block);
    let rules = prop.rules;
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules,
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_proposals(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn reverse_proposals(
    deps: Deps,
    env: Env,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn map_proposal(
    block: &BlockInfo,
    item: StdResult<(Vec<u8>, Proposal)>,
) -> StdResult<ProposalResponse> {
    let (key, prop) = item?;
    let status = prop.current_status(block);
    Ok(ProposalResponse {
        id: parse_id(&key)?,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules: prop.rules,
    })
}

fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = BALLOTS.may_load(deps.storage, (proposal_id.into(), &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        voter,
        vote: b.vote,
        weight: b.weight,
    });
    Ok(VoteResponse { vote })
}

fn list_votes(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let votes: StdResult<Vec<_>> = BALLOTS
        .prefix(proposal_id.into())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (voter, ballot) = item?;
            Ok(VoteInfo {
                voter: String::from_utf8(voter)?,
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

fn query_voter(deps: Deps, voter: String) -> StdResult<VoterResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voter_addr = deps.api.addr_validate(&voter)?;
    let weight = cfg
        .engagement_contract
        .is_member(&deps.querier, &voter_addr)?;

    Ok(VoterResponse { weight })
}

fn list_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voters = cfg
        .engagement_contract
        .list_members(&deps.querier, start_after, limit)?
        .into_iter()
        .map(|member| VoterDetail {
            addr: member.addr,
            weight: member.weight,
        })
        .collect();
    Ok(VoterListResponse { voters })
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{coin, coins, Addr, Coin, Decimal, Timestamp};

    use cw0::Duration;
    use cw_multi_test::{next_block, Contract, ContractWrapper, Executor};
    use tg4::{Member, Tg4ExecuteMsg, Tg4QueryMsg};
    use tg_bindings_test::TgradeApp;

    use super::*;

    const OWNER: &str = "admin0001";
    const VOTER1: &str = "voter0001";
    const VOTER2: &str = "voter0002";
    const VOTER3: &str = "voter0003";
    const VOTER4: &str = "voter0004";
    const VOTER5: &str = "voter0005";
    const SOMEBODY: &str = "somebody";

    const ENGAGEMENT_TOKEN: &str = "engagement";

    fn member<T: Into<String>>(addr: T, weight: u64) -> Member {
        Member {
            addr: addr.into(),
            weight,
        }
    }

    pub fn contract_flex() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new(
            tg4_engagement::contract::execute,
            tg4_engagement::contract::instantiate,
            tg4_engagement::contract::query,
        );
        Box::new(contract)
    }

    fn mock_app(init_funds: &[Coin]) -> TgradeApp {
        let mut app = TgradeApp::new(OWNER);
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &Addr::unchecked(OWNER), init_funds.to_vec())
                .unwrap();
        });
        app
    }

    // uploads code and returns address of engagement contract
    fn instantiate_engagement(
        app: &mut TgradeApp,
        admin: Option<String>,
        members: Vec<Member>,
    ) -> Addr {
        let engagement_id = app.store_code(contract_engagement());
        let msg = tg4_engagement::msg::InstantiateMsg {
            admin,
            members,
            preauths: None,
            halflife: None,
            token: ENGAGEMENT_TOKEN.to_owned(),
        };
        app.instantiate_contract(
            engagement_id,
            Addr::unchecked(OWNER),
            &msg,
            &[],
            "engagement",
            None,
        )
        .unwrap()
    }

    fn instantiate_flex(
        app: &mut TgradeApp,
        engagement_contract: Addr,
        threshold: Threshold,
        max_voting_period: Duration,
    ) -> Addr {
        let flex_id = app.store_code(contract_flex());
        let msg = crate::msg::InstantiateMsg {
            threshold,
            max_voting_period,
            engagement_contract: engagement_contract.to_string(),
        };
        app.instantiate_contract(flex_id, Addr::unchecked(OWNER), &msg, &[], "flex", None)
            .unwrap()
    }

    // this will set up both contracts, instantiating the group with
    // all voters defined above, and the multisig pointing to it and given threshold criteria.
    // Returns (multisig address, group address).
    fn setup_test_case_fixed(
        app: &mut TgradeApp,
        rules: VotingRules,
        init_funds: Vec<Coin>,
        multisig_as_group_admin: bool,
    ) -> (Addr, Addr) {
        setup_test_case(app, rules, init_funds, multisig_as_group_admin)
    }

    fn setup_test_case(
        app: &mut TgradeApp,
        rules: VotingRules,
        init_funds: Vec<Coin>,
        multisig_as_group_admin: bool,
    ) -> (Addr, Addr) {
        // 1. Instantiate group engagement contract with members (and OWNER as admin)
        let members = vec![
            member(OWNER, 0),
            member(VOTER1, 1),
            member(VOTER2, 2),
            member(VOTER3, 3),
            member(VOTER4, 4),
            member(VOTER5, 5),
        ];
        let engagement_addr = instantiate_engagement(app, Some(OWNER.to_string()), members);
        app.update_block(next_block);

        // 2. Set up Multisig backed by this group
        let flex_addr =
            instantiate_flex(app, engagement_addr.clone(), threshold, max_voting_period);

        // 2.5 Set flex contract's address as admin of engagement contract
        app.execute_contract(
            Addr::unchecked(OWNER),
            engagement_addr.clone(),
            &Tg4ExecuteMsg::UpdateAdmin {
                admin: Some(flex_addr.to_string()),
            },
            &[],
        )
        .unwrap();
        app.update_block(next_block);

        // 3. (Optional) Set the multisig as the group owner
        if multisig_as_group_admin {
            let update_admin = Tg4ExecuteMsg::UpdateAdmin {
                admin: Some(flex_addr.to_string()),
            };
            app.execute_contract(
                flex_addr.clone(),
                engagement_addr.clone(),
                &update_admin,
                &[],
            )
            .unwrap();
            app.update_block(next_block);
        }

        // Bonus: set some funds on the multisig contract for future proposals
        if !init_funds.is_empty() {
            app.send_tokens(Addr::unchecked(OWNER), flex_addr.clone(), &init_funds)
                .unwrap();
        }
        (flex_addr, engagement_addr)
    }

    fn proposal_info() -> (OversightProposal, String, String) {
        let proposal = OversightProposal::GrantEngagement {
            member: Addr::unchecked(VOTER1),
            points: 10,
        };
        let title = "Grant engagement point to somebody".to_string();
        let description = "Did I grant him?".to_string();
        (proposal, title, description)
    }

    fn grant_voter1_engagement_point_proposal() -> ExecuteMsg {
        let (proposal, title, description) = proposal_info();
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
            latest: None,
        }
    }

    #[test]
    fn test_instantiate_works() {
        let mut app = mock_app(&[]);

        // make a simple group
        let flex_id = app.store_code(contract_flex());
        let engagement_addr =
            instantiate_engagement(&mut app, Some(OWNER.to_string()), vec![member(OWNER, 1)]);

        let max_voting_period = Duration::Time(1234567);

        // Zero required weight fails
        let instantiate_msg = InstantiateMsg {
            threshold: Threshold::AbsoluteCount { weight: 0 },
            max_voting_period,
            engagement_contract: engagement_addr.to_string(),
        };
        let err = app
            .instantiate_contract(
                flex_id,
                Addr::unchecked(OWNER),
                &instantiate_msg,
                &[],
                "zero required weight",
                None,
            )
            .unwrap_err();
        assert_eq!(ContractError::ZeroThreshold {}, err.downcast().unwrap());

        // Total weight less than required weight not allowed
        let instantiate_msg = InstantiateMsg {
            threshold: Threshold::AbsoluteCount { weight: 100 },
            max_voting_period,
            engagement_contract: engagement_addr.to_string(),
        };
        let err = app
            .instantiate_contract(
                flex_id,
                Addr::unchecked(OWNER),
                &instantiate_msg,
                &[],
                "high required weight",
                None,
            )
            .unwrap_err();
        assert_eq!(
            ContractError::UnreachableThreshold {},
            err.downcast().unwrap()
        );

        // All valid
        let instantiate_msg = InstantiateMsg {
            threshold: Threshold::AbsoluteCount { weight: 1 },
            max_voting_period,
            engagement_contract: engagement_addr.to_string(),
        };
        let flex_addr = app
            .instantiate_contract(
                flex_id,
                Addr::unchecked(OWNER),
                &instantiate_msg,
                &[],
                "all good",
                None,
            )
            .unwrap();

        // Get voters query
        let voters: VoterListResponse = app
            .wrap()
            .query_wasm_smart(
                &flex_addr,
                &QueryMsg::ListVoters {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(
            voters.voters,
            vec![VoterDetail {
                addr: OWNER.into(),
                weight: 1
            }]
        );
    }

    #[test]
    fn test_propose_works() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        let (flex_addr, _) = setup_test_case_fixed(
            &mut app,
            mock_rules().threshold(Decimal::percent(25)).build(),
            init_funds,
            false,
        );

        let proposal_msg = grant_voter1_engagement_point_proposal();
        // Only voters can propose
        let err = app
            .execute_contract(
                Addr::unchecked(SOMEBODY),
                flex_addr.clone(),
                &proposal_msg,
                &[],
            )
            .unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

        // Wrong expiration option fails
        let proposal = match proposal_msg.clone() {
            ExecuteMsg::Propose { proposal, .. } => proposal,
            _ => panic!("Wrong variant"),
        };
        let proposal_wrong_exp = ExecuteMsg::Propose {
            title: "Rewarding somebody".to_string(),
            description: "Do we reward her?".to_string(),
            proposal,
            latest: Some(Expiration::AtHeight(123456)),
        };
        let err = app
            .execute_contract(
                Addr::unchecked(OWNER),
                flex_addr.clone(),
                &proposal_wrong_exp,
                &[],
            )
            .unwrap_err();
        assert_eq!(ContractError::WrongExpiration {}, err.downcast().unwrap());

        // Proposal from voter works
        let res = app
            .execute_contract(
                Addr::unchecked(VOTER3),
                flex_addr.clone(),
                &proposal_msg,
                &[],
            )
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "propose"),
                ("sender", VOTER3),
                ("proposal_id", "1"),
                ("status", "Open"),
            ],
        );

        // Proposal from voter with enough vote power directly passes
        let res = app
            .execute_contract(Addr::unchecked(VOTER4), flex_addr, &proposal_msg, &[])
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "propose"),
                ("sender", VOTER4),
                ("proposal_id", "2"),
                ("status", "Passed"),
            ],
        );
    }

    fn get_tally(app: &TgradeApp, flex_addr: &str, proposal_id: u64) -> u64 {
        // Get all the voters on the proposal
        let voters = QueryMsg::ListVotes {
            proposal_id,
            start_after: None,
            limit: None,
        };
        let votes: VoteListResponse = app.wrap().query_wasm_smart(flex_addr, &voters).unwrap();
        // Sum the weights of the Yes votes to get the tally
        votes
            .votes
            .iter()
            .filter(|&v| v.vote == Vote::Yes)
            .map(|v| v.weight)
            .sum()
    }

    fn expire(voting_period: Duration) -> impl Fn(&mut BlockInfo) {
        move |block: &mut BlockInfo| {
            match voting_period {
                Duration::Time(duration) => block.time = block.time.plus_seconds(duration + 1),
                Duration::Height(duration) => block.height += duration + 1,
            };
        }
    }

    fn unexpire(voting_period: Duration) -> impl Fn(&mut BlockInfo) {
        move |block: &mut BlockInfo| {
            match voting_period {
                Duration::Time(duration) => {
                    block.time =
                        Timestamp::from_nanos(block.time.nanos() - (duration * 1_000_000_000));
                }
                Duration::Height(duration) => block.height -= duration,
            };
        }
    }

    #[test]
    fn test_proposal_queries() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        let rules = mock_rules()
            .quorum(Decimal::percent(20))
            .threshold(Decimal::percent(20))
            .build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, _) = setup_test_case_fixed(&mut app, rules.clone(), init_funds, false);

        // create proposal with 1 vote power
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER1), flex_addr.clone(), &proposal, &[])
            .unwrap();
        let proposal_id1: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // another proposal immediately passes
        app.update_block(next_block);
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &proposal, &[])
            .unwrap();
        let proposal_id2: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // expire them both
        app.update_block(expire(voting_period));

        // add one more open proposal, 2 votes
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &proposal, &[])
            .unwrap();
        let proposal_id3: u64 = res.custom_attrs(1)[2].value.parse().unwrap();
        let proposed_at = app.block_info();

        // next block, let's query them all... make sure status is properly updated (1 should be rejected in query)
        app.update_block(next_block);
        let list_query = QueryMsg::ListProposals {
            start_after: None,
            limit: None,
        };
        let res: ProposalListResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &list_query)
            .unwrap();
        assert_eq!(3, res.proposals.len());

        // check the id and status are properly set
        let info: Vec<_> = res.proposals.iter().map(|p| (p.id, p.status)).collect();
        let expected_info = vec![
            (proposal_id1, Status::Rejected),
            (proposal_id2, Status::Passed),
            (proposal_id3, Status::Open),
        ];
        assert_eq!(expected_info, info);

        // ensure the common features are set
        let (expected_proposal, expected_title, expected_description) = proposal_info();
        for prop in res.proposals {
            assert_eq!(prop.title, expected_title);
            assert_eq!(prop.description, expected_description);
            assert_eq!(prop.proposal, expected_proposal);
        }

        // reverse query can get just proposal_id3
        let list_query = QueryMsg::ReverseProposals {
            start_before: None,
            limit: Some(1),
        };
        let res: ProposalListResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &list_query)
            .unwrap();
        assert_eq!(1, res.proposals.len());

        let (proposal, title, description) = proposal_info();
        let expected = ProposalResponse {
            id: proposal_id3,
            title,
            description,
            proposal,
            expires: voting_period.after(&proposed_at),
            status: Status::Open,
            rules,
        };
        assert_eq!(&expected, &res.proposals[0]);
    }

    #[test]
    fn test_vote_works() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        let rules = mock_rules().threshold(Decimal::percent(30)).build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, _) = setup_test_case_fixed(&mut app, rules, init_funds, false);

        // create proposal with 0 vote power
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr.clone(), &proposal, &[])
            .unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // Owner cannot vote (again)
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        let err = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr.clone(), &yes_vote, &[])
            .unwrap_err();
        assert_eq!(ContractError::AlreadyVoted {}, err.downcast().unwrap());

        // Only voters can vote
        let err = app
            .execute_contract(Addr::unchecked(SOMEBODY), flex_addr.clone(), &yes_vote, &[])
            .unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

        // But voter1 can
        let res = app
            .execute_contract(Addr::unchecked(VOTER1), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "vote"),
                ("sender", VOTER1),
                ("proposal_id", proposal_id.to_string().as_str()),
                ("status", "Open"),
            ],
        );

        // No/Veto votes have no effect on the tally
        // Compute the current tally
        let tally = get_tally(&app, flex_addr.as_ref(), proposal_id);
        assert_eq!(tally, 1);

        // Cast a No vote
        let no_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::No,
        };
        let _ = app
            .execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &no_vote, &[])
            .unwrap();

        // Cast a Veto vote
        let veto_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Veto,
        };
        let _ = app
            .execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &veto_vote, &[])
            .unwrap();

        // Tally unchanged
        assert_eq!(tally, get_tally(&app, flex_addr.as_ref(), proposal_id));

        let err = app
            .execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &yes_vote, &[])
            .unwrap_err();
        assert_eq!(ContractError::AlreadyVoted {}, err.downcast().unwrap());

        // Expired proposals cannot be voted
        app.update_block(expire(voting_period));
        let err = app
            .execute_contract(Addr::unchecked(VOTER4), flex_addr.clone(), &yes_vote, &[])
            .unwrap_err();
        assert_eq!(ContractError::Expired {}, err.downcast().unwrap());
        app.update_block(unexpire(voting_period));

        // Powerful voter supports it, so it passes
        let res = app
            .execute_contract(Addr::unchecked(VOTER4), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "vote"),
                ("sender", VOTER4),
                ("proposal_id", proposal_id.to_string().as_str()),
                ("status", "Passed"),
            ],
        );

        // non-Open proposals cannot be voted
        let err = app
            .execute_contract(Addr::unchecked(VOTER5), flex_addr.clone(), &yes_vote, &[])
            .unwrap_err();
        assert_eq!(ContractError::NotOpen {}, err.downcast().unwrap());

        // query individual votes
        // initial (with 0 weight)
        let voter = OWNER.into();
        let vote: VoteResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &QueryMsg::Vote { proposal_id, voter })
            .unwrap();
        assert_eq!(
            vote.vote.unwrap(),
            VoteInfo {
                voter: OWNER.into(),
                vote: Vote::Yes,
                weight: 0
            }
        );

        // nay sayer
        let voter = VOTER2.into();
        let vote: VoteResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &QueryMsg::Vote { proposal_id, voter })
            .unwrap();
        assert_eq!(
            vote.vote.unwrap(),
            VoteInfo {
                voter: VOTER2.into(),
                vote: Vote::No,
                weight: 2
            }
        );

        // non-voter
        let voter = VOTER5.into();
        let vote: VoteResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &QueryMsg::Vote { proposal_id, voter })
            .unwrap();
        assert!(vote.vote.is_none());
    }

    #[test]
    fn test_execute_works() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        let rules = mock_rules().threshold(Decimal::percent(20)).build();
        let (flex_addr, _) = setup_test_case_fixed(&mut app, rules, init_funds, true);

        // ensure we have cash to cover the proposal
        let contract_bal = app.wrap().query_balance(&flex_addr, "BTC").unwrap();
        assert_eq!(contract_bal, coin(10, "BTC"));

        // create proposal with 0 vote power
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr.clone(), &proposal, &[])
            .unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // Only Passed can be executed
        let execution = ExecuteMsg::Execute { proposal_id };
        let err = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr.clone(), &execution, &[])
            .unwrap_err();
        assert_eq!(
            ContractError::WrongExecuteStatus {},
            err.downcast().unwrap()
        );

        // Vote it, so it passes
        let vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        let res = app
            .execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &vote, &[])
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "vote"),
                ("sender", VOTER3),
                ("proposal_id", proposal_id.to_string().as_str()),
                ("status", "Passed"),
            ],
        );

        // In passing: Try to close Passed fails
        let closing = ExecuteMsg::Close { proposal_id };
        let err = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr.clone(), &closing, &[])
            .unwrap_err();
        assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());

        // Execute works. Anybody can execute Passed proposals
        let res = app
            .execute_contract(
                Addr::unchecked(SOMEBODY),
                flex_addr.clone(),
                &execution,
                &[],
            )
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "execute"),
                ("sender", SOMEBODY),
                ("proposal_id", proposal_id.to_string().as_str()),
            ],
        );

        // verify engagement points were transfered
        // engagement_contract is initialized with members
        // Member VOTER1 has 1 point of weight, after proposal it
        // should be 11
        let engagement_points: tg4::MemberResponse = app
            .wrap()
            .query_wasm_smart(
                engagement_addr,
                &Tg4QueryMsg::Member {
                    addr: VOTER1.to_string(),
                    at_height: None,
                },
            )
            .unwrap();
        assert_eq!(engagement_points.weight.unwrap(), 11);

        // In passing: Try to close Executed fails
        let err = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr, &closing, &[])
            .unwrap_err();
        assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());
    }

    #[test]
    fn test_close_works() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        let rules = mock_rules().threshold(Decimal::percent(20)).build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, _) = setup_test_case_fixed(&mut app, rules, init_funds, true);

        // create proposal with 0 vote power
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(OWNER), flex_addr.clone(), &proposal, &[])
            .unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // Non-expired proposals cannot be closed
        let closing = ExecuteMsg::Close { proposal_id };
        let err = app
            .execute_contract(Addr::unchecked(SOMEBODY), flex_addr.clone(), &closing, &[])
            .unwrap_err();
        assert_eq!(ContractError::NotExpired {}, err.downcast().unwrap());

        // Expired proposals can be closed
        app.update_block(expire(voting_period));
        let res = app
            .execute_contract(Addr::unchecked(SOMEBODY), flex_addr.clone(), &closing, &[])
            .unwrap();
        assert_eq!(
            res.custom_attrs(1),
            [
                ("action", "close"),
                ("sender", SOMEBODY),
                ("proposal_id", proposal_id.to_string().as_str()),
            ],
        );

        // Trying to close it again fails
        let closing = ExecuteMsg::Close { proposal_id };
        let err = app
            .execute_contract(Addr::unchecked(SOMEBODY), flex_addr, &closing, &[])
            .unwrap_err();
        assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());
    }

    // uses the power from the beginning of the voting period
    #[test]
    fn execute_group_changes_from_external() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        let rules = mock_rules().threshold(Decimal::percent(30)).build();
        let (flex_addr, group_addr) = setup_test_case_fixed(&mut app, rules, init_funds, false);

        // VOTER1 starts a proposal to send some tokens (1/4 votes)
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER1), flex_addr.clone(), &proposal, &[])
            .unwrap();
        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();
        let prop_status = |app: &TgradeApp, proposal_id: u64| -> Status {
            let query_prop = QueryMsg::Proposal { proposal_id };
            let prop: ProposalResponse = app
                .wrap()
                .query_wasm_smart(&flex_addr, &query_prop)
                .unwrap();
            prop.status
        };

        // 1/4 votes
        assert_eq!(prop_status(&app, proposal_id), Status::Open);

        // a few blocks later...
        app.update_block(|block| block.height += 2);

        // admin changes the group
        // updates VOTER2 power to 7 -> with snapshot, vote doesn't pass proposal
        // adds NEWBIE with 2 power -> with snapshot, invalid vote
        // removes VOTER3 -> with snapshot, can vote and pass proposal
        let newbie: &str = "newbie";
        let update_msg = tg4_engagement::msg::ExecuteMsg::UpdateMembers {
            remove: vec![VOTER3.into()],
            add: vec![member(VOTER2, 7), member(newbie, 2)],
        };
        app.execute_contract(
            Addr::unchecked(flex_addr.clone()),
            engagement_addr,
            &update_msg,
            &[],
        )
        .unwrap();

        // check membership queries properly updated
        let query_voter = QueryMsg::Voter {
            address: VOTER3.into(),
        };
        let power: VoterResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &query_voter)
            .unwrap();
        assert_eq!(power.weight, None);

        // proposal still open
        assert_eq!(prop_status(&app, proposal_id), Status::Open);

        // a few blocks later...
        app.update_block(|block| block.height += 3);

        // make a second proposal
        let proposal2 = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER1), flex_addr.clone(), &proposal2, &[])
            .unwrap();
        // Get the proposal id from the logs
        let proposal_id2: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // VOTER2 can pass this alone with the updated vote (newer height ignores snapshot)
        let yes_vote = ExecuteMsg::Vote {
            proposal_id: proposal_id2,
            vote: Vote::Yes,
        };
        app.execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(prop_status(&app, proposal_id2), Status::Passed);

        // VOTER2 can only vote on first proposal with weight of 2 (not enough to pass)
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        app.execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(prop_status(&app, proposal_id), Status::Open);

        // newbie cannot vote
        let err = app
            .execute_contract(Addr::unchecked(newbie), flex_addr.clone(), &yes_vote, &[])
            .unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

        // previously removed VOTER3 can still vote, passing the proposal
        app.execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(prop_status(&app, proposal_id), Status::Passed);
    }

    // uses the power from the beginning of the voting period
    #[test]
    fn percentage_handles_group_changes() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        // 33% required, which is 5 of the initial 15
        let rules = mock_rules().threshold(Decimal::percent(33)).build();
        let (flex_addr, group_addr) = setup_test_case(&mut app, rules, init_funds, false);

        // VOTER3 starts a proposal to send some tokens (3/5 votes)
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &proposal, &[])
            .unwrap();
        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();
        let prop_status = |app: &TgradeApp| -> Status {
            let query_prop = QueryMsg::Proposal { proposal_id };
            let prop: ProposalResponse = app
                .wrap()
                .query_wasm_smart(&flex_addr, &query_prop)
                .unwrap();
            prop.status
        };

        // 3/5 votes
        assert_eq!(prop_status(&app), Status::Open);

        // a few blocks later...
        app.update_block(|block| block.height += 2);

        // admin changes the group (3 -> 0, 2 -> 7, 0 -> 15) - total = 32, require 11 to pass
        let newbie: &str = "newbie";
        let update_msg = tg4_engagement::msg::ExecuteMsg::UpdateMembers {
            remove: vec![VOTER3.into()],
            add: vec![member(VOTER2, 7), member(newbie, 15)],
        };
        app.execute_contract(
            Addr::unchecked(flex_addr.clone()),
            engagement_addr,
            &update_msg,
            &[],
        )
        .unwrap();

        // a few blocks later...
        app.update_block(|block| block.height += 3);

        // VOTER2 votes according to original weights: 3 + 2 = 5 / 5 => Passed
        // with updated weights, it would be 3 + 7 = 10 / 11 => Open
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        app.execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(prop_status(&app), Status::Passed);

        // new proposal can be passed single-handedly by newbie
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(newbie), flex_addr.clone(), &proposal, &[])
            .unwrap();
        // Get the proposal id from the logs
        let proposal_id2: u64 = res.custom_attrs(1)[2].value.parse().unwrap();

        // check proposal2 status
        let query_prop = QueryMsg::Proposal {
            proposal_id: proposal_id2,
        };
        let prop: ProposalResponse = app
            .wrap()
            .query_wasm_smart(&flex_addr, &query_prop)
            .unwrap();
        assert_eq!(Status::Passed, prop.status);
    }

    // uses the power from the beginning of the voting period
    #[test]
    fn quorum_handles_group_changes() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        // 33% required for quora, which is 5 of the initial 15
        // 50% yes required to pass early (8 of the initial 15)
        let rules = mock_rules()
            .threshold(Decimal::percent(50))
            .quorum(Decimal::percent(33))
            .build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, group_addr) = setup_test_case(&mut app, rules, init_funds, false);

        // VOTER3 starts a proposal to send some tokens (3 votes)
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &proposal, &[])
            .unwrap();
        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();
        let prop_status = |app: &TgradeApp| -> Status {
            let query_prop = QueryMsg::Proposal { proposal_id };
            let prop: ProposalResponse = app
                .wrap()
                .query_wasm_smart(&flex_addr, &query_prop)
                .unwrap();
            prop.status
        };

        // 3/5 votes - not expired
        assert_eq!(prop_status(&app), Status::Open);

        // a few blocks later...
        app.update_block(|block| block.height += 2);

        // admin changes the group (3 -> 0, 2 -> 7, 0 -> 15) - total = 32, require 11 to pass
        let newbie: &str = "newbie";
        let update_msg = tg4_engagement::msg::ExecuteMsg::UpdateMembers {
            remove: vec![VOTER3.into()],
            add: vec![member(VOTER2, 7), member(newbie, 15)],
        };
        app.execute_contract(
            Addr::unchecked(flex_addr.clone()),
            engagement_addr,
            &update_msg,
            &[],
        )
        .unwrap();

        // a few blocks later...
        app.update_block(|block| block.height += 3);

        // VOTER2 votes no, according to original weights: 3 yes, 2 no, 5 total (will pass when expired)
        // with updated weights, it would be 3 yes, 7 no, 10 total (will fail when expired)
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::No,
        };
        app.execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        // not expired yet
        assert_eq!(prop_status(&app), Status::Open);

        // wait until the vote is over, and see it was passed (met quorum, and threshold of voters)
        app.update_block(expire(voting_period));
        assert_eq!(prop_status(&app), Status::Passed);
    }

    #[test]
    fn quorum_enforced_even_if_absolute_threshold_met() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        // 33% required for quora, which is 5 of the initial 15
        // 50% yes required to pass early (8 of the initial 15)
        let rules = mock_rules()
            .threshold(Decimal::percent(60))
            .quorum(Decimal::percent(80))
            .build();
        let (flex_addr, _) = setup_test_case(
            &mut app, // note that 60% yes is not enough to pass without 20% no as well
            rules, init_funds, false,
        );

        // create proposal
        let proposal = grant_voter1_engagement_point_proposal();
        let res = app
            .execute_contract(Addr::unchecked(VOTER5), flex_addr.clone(), &proposal, &[])
            .unwrap();
        // Get the proposal id from the logs
        let proposal_id: u64 = res.custom_attrs(1)[2].value.parse().unwrap();
        let prop_status = |app: &TgradeApp| -> Status {
            let query_prop = QueryMsg::Proposal { proposal_id };
            let prop: ProposalResponse = app
                .wrap()
                .query_wasm_smart(&flex_addr, &query_prop)
                .unwrap();
            prop.status
        };
        assert_eq!(prop_status(&app), Status::Open);
        app.update_block(|block| block.height += 3);

        // reach 60% of yes votes, not enough to pass early (or late)
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        app.execute_contract(Addr::unchecked(VOTER4), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        // 9 of 15 is 60% absolute threshold, but less than 12 (80% quorum needed)
        assert_eq!(prop_status(&app), Status::Open);

        // add 3 weight no vote and we hit quorum and this passes
        let no_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::No,
        };
        app.execute_contract(Addr::unchecked(VOTER3), flex_addr.clone(), &no_vote, &[])
            .unwrap();
        assert_eq!(prop_status(&app), Status::Passed);
    }
}
