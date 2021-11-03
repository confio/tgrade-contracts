#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, BlockInfo, Deps, DepsMut, Env, MessageInfo, Order, StdResult,
};

use cw0::{maybe_addr, Expiration};
use cw2::set_contract_version;
use cw3::{
    Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse,
    VoterResponse,
};
use cw_storage_plus::Bound;
use tg4::Tg4Contract;
use tg_bindings::TgradeMsg;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{
    next_id, parse_id, Ballot, Config, OversightProposal, Proposal, ProposalListResponse,
    ProposalResponse, Votes, VotingRules, BALLOTS, CONFIG, PROPOSALS,
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
    let group_contract = Tg4Contract(deps.api.addr_validate(&msg.group_addr).map_err(|_| {
        ContractError::InvalidGroup {
            addr: msg.group_addr.clone(),
        }
    })?);

    let engagement_contract =
        Tg4Contract(deps.api.addr_validate(&msg.engagement_addr).map_err(|_| {
            ContractError::InvalidEngagementContract {
                addr: msg.engagement_addr.clone(),
            }
        })?);
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cfg = Config {
        rules: msg.rules,
        group_contract,
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
        } => execute_propose(deps, env, info, title, description, proposal),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
    }
}

fn confirm_voting_power(deps: Deps, member: &Addr, height: u64) -> Result<(), ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    cfg.group_contract
        .member_at_height(&deps.querier, member, height)?
        .map_or_else(
            || Err(ContractError::Unauthorized {}),
            |w| {
                if w < 1 {
                    Err(ContractError::Unauthorized {})
                } else {
                    Ok(())
                }
            },
        )
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: OversightProposal,
) -> Result<Response, ContractError> {
    // only members of the multisig can create a proposal
    let cfg = CONFIG.load(deps.storage)?;

    let vote_power = cfg
        .group_contract
        .is_member(&deps.querier, &info.sender)?
        .ok_or(ContractError::Unauthorized {})?;

    // Additional check if weight >= 1
    confirm_voting_power(deps.as_ref(), &info.sender, env.block.height)?;

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
        votes: Votes::yes(vote_power),
        rules: cfg.rules,
        total_weight: cfg.group_contract.total_weight(&deps.querier)?,
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
        .group_contract
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

    let message = match prop.proposal {
        OversightProposal::GrantEngagement { ref member, points } => engagement_contract
            .encode_raw_msg(to_binary(&tg4_engagement::ExecuteMsg::AddPoints {
                addr: member.to_string(),
                points,
            })?)?,
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
    let weight = cfg.group_contract.is_member(&deps.querier, &voter_addr)?;

    Ok(VoterResponse { weight })
}

fn list_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voters = cfg
        .group_contract
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
    use cosmwasm_std::{coins, Addr, Coin, Decimal, Timestamp};

    use cw0::Duration;
    use cw_multi_test::{next_block, Contract, ContractWrapper, Executor};
    use tg4::{Member, Tg4ExecuteMsg};
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

    // uploads code and returns address of group contract
    fn instantiate_group(app: &mut TgradeApp, members: Vec<Member>) -> Addr {
        let group_id = app.store_code(contract_engagement());
        let msg = tg4_engagement::msg::InstantiateMsg {
            admin: Some(OWNER.into()),
            members,
            preauths: 0,
            preauths_slashing: 0,
            halflife: None,
            token: ENGAGEMENT_TOKEN.to_owned(),
        };
        app.instantiate_contract(group_id, Addr::unchecked(OWNER), &msg, &[], "group", None)
            .unwrap()
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
            preauths: 0,
            preauths_slashing: 0,
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

    #[track_caller]
    fn instantiate_flex(
        app: &mut TgradeApp,
        group: Addr,
        engagement: Addr,
        rules: VotingRules,
    ) -> Addr {
        let flex_id = app.store_code(contract_flex());
        let msg = crate::msg::InstantiateMsg {
            group_addr: group.to_string(),
            engagement_addr: engagement.to_string(),
            rules,
        };
        app.instantiate_contract(flex_id, Addr::unchecked(OWNER), &msg, &[], "flex", None)
            .unwrap()
    }

    // this will set up both contracts, instantiating the group with
    // all voters defined above, and the multisig pointing to it and given threshold criteria.
    // Returns (multisig address, group address).
    #[track_caller]
    fn setup_test_case_fixed(
        app: &mut TgradeApp,
        rules: VotingRules,
        init_funds: Vec<Coin>,
        multisig_as_group_admin: bool,
    ) -> (Addr, Addr, Addr) {
        setup_test_case(app, rules, init_funds, multisig_as_group_admin)
    }

    #[track_caller]
    fn setup_test_case(
        app: &mut TgradeApp,
        rules: VotingRules,
        init_funds: Vec<Coin>,
        multisig_as_group_admin: bool,
    ) -> (Addr, Addr, Addr) {
        // 1. Instantiate group engagement contract with members (and OWNER as admin)
        let members = vec![
            member(OWNER, 0),
            member(VOTER1, 1),
            member(VOTER2, 2),
            member(VOTER3, 3),
            member(VOTER4, 12), // so that he alone can pass a 50 / 52% threshold proposal
            member(VOTER5, 5),
        ];
        let group_addr = instantiate_group(app, members.clone());
        let engagement_addr = instantiate_engagement(app, Some(OWNER.to_string()), members);
        app.update_block(next_block);

        // 2. Set up Multisig backed by this group
        let flex_addr = instantiate_flex(app, group_addr.clone(), engagement_addr.clone(), rules);

        // 2.5 Set oc proposals contract's address as admin of engagement contract
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
                Addr::unchecked(OWNER),
                group_addr.clone(),
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
        (flex_addr, group_addr, engagement_addr)
    }

    struct MockRulesBuilder {
        pub voting_period: u32,
        pub quorum: Decimal,
        pub threshold: Decimal,
        pub allow_end_early: bool,
    }

    impl MockRulesBuilder {
        fn new() -> Self {
            Self {
                voting_period: 14,
                quorum: Decimal::percent(1),
                threshold: Decimal::percent(50),
                allow_end_early: true,
            }
        }

        fn quorum(&mut self, quorum: impl Into<Decimal>) -> &mut Self {
            self.quorum = quorum.into();
            self
        }

        fn threshold(&mut self, threshold: impl Into<Decimal>) -> &mut Self {
            self.threshold = threshold.into();
            self
        }

        fn build(&self) -> VotingRules {
            VotingRules {
                voting_period: self.voting_period,
                quorum: self.quorum,
                threshold: self.threshold,
                allow_end_early: self.allow_end_early,
            }
        }
    }

    fn mock_rules() -> MockRulesBuilder {
        MockRulesBuilder::new()
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
        }
    }

    #[test]
    fn test_instantiate_works() {
        let mut app = mock_app(&[]);

        // make a simple group
        let group_addr = instantiate_group(&mut app, vec![member(OWNER, 1)]);
        let engagement_addr =
            instantiate_engagement(&mut app, Some(OWNER.to_string()), vec![member(OWNER, 1)]);
        let flex_id = app.store_code(contract_flex());

        // Zero required weight fails
        let instantiate_msg = InstantiateMsg {
            group_addr: group_addr.to_string(),
            engagement_addr: engagement_addr.to_string(),
            rules: mock_rules().threshold(Decimal::zero()).build(),
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
        assert_eq!(
            ContractError::InvalidThreshold(Decimal::zero()),
            err.downcast().unwrap()
        );

        // All valid
        let instantiate_msg = InstantiateMsg {
            group_addr: group_addr.to_string(),
            engagement_addr: engagement_addr.to_string(),
            rules: mock_rules().build(),
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
            .threshold(Decimal::percent(80))
            .build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, _, _) = setup_test_case_fixed(&mut app, rules.clone(), init_funds, false);

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
            .execute_contract(Addr::unchecked(VOTER4), flex_addr.clone(), &proposal, &[])
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

        let rules = mock_rules().threshold(Decimal::percent(51)).build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, _, _) = setup_test_case_fixed(&mut app, rules, init_funds, false);

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

    // uses the power from the beginning of the voting period
    #[test]
    fn percentage_handles_group_changes() {
        let init_funds = coins(10, "BTC");
        let mut app = mock_app(&init_funds);

        // 51% required, which is 12 of the initial 23
        let rules = mock_rules().threshold(Decimal::percent(51)).build();
        let (flex_addr, group_addr, _) = setup_test_case(&mut app, rules, init_funds, false);

        // VOTER3 starts a proposal to send some tokens (3/12 votes)
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

        // 3/12 votes
        assert_eq!(prop_status(&app), Status::Open);

        // a few blocks later...
        app.update_block(|block| block.height += 2);

        // admin changes the group (3 -> 0, 2 -> 9, 0 -> 29) - total = 56, require 29 to pass
        let newbie: &str = "newbie";
        let update_msg = tg4_engagement::msg::ExecuteMsg::UpdateMembers {
            remove: vec![VOTER3.into()],
            add: vec![member(VOTER2, 9), member(newbie, 29)],
        };
        app.execute_contract(Addr::unchecked(OWNER), group_addr, &update_msg, &[])
            .unwrap();

        // a few blocks later...
        app.update_block(|block| block.height += 3);

        // VOTER2 votes according to original weights: 3 + 2 = 5 / 12 => Open
        // with updated weights, it would be 3 + 9 = 12 / 12 => Passed
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        app.execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        assert_eq!(prop_status(&app), Status::Open);

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

        // 33% required for quora, which is 8 of the initial 23
        // 50% yes required to pass early (12 of the initial 23)
        let rules = mock_rules()
            .threshold(Decimal::percent(50))
            .quorum(Decimal::percent(33))
            .build();
        let voting_period = Duration::Time(rules.voting_period_secs());
        let (flex_addr, group_addr, _) = setup_test_case(&mut app, rules, init_funds, false);

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

        // 3/12 votes - not expired
        assert_eq!(prop_status(&app), Status::Open);

        // a few blocks later...
        app.update_block(|block| block.height += 2);

        // admin changes the group (3 -> 0, 2 -> 9, 0 -> 28) - total = 55, require 28 to pass
        let newbie: &str = "newbie";
        let update_msg = tg4_engagement::msg::ExecuteMsg::UpdateMembers {
            remove: vec![VOTER3.into()],
            add: vec![member(VOTER2, 9), member(newbie, 28)],
        };
        app.execute_contract(Addr::unchecked(OWNER), group_addr, &update_msg, &[])
            .unwrap();

        // a few blocks later...
        app.update_block(|block| block.height += 3);

        // VOTER2 votes yes, according to original weights: 3 yes, 2 no, 5 total (will fail when expired)
        // with updated weights, it would be 3 yes, 9 yes, 11 total (will pass when expired)
        let yes_vote = ExecuteMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        app.execute_contract(Addr::unchecked(VOTER2), flex_addr.clone(), &yes_vote, &[])
            .unwrap();
        // not expired yet
        assert_eq!(prop_status(&app), Status::Open);

        // wait until the vote is over, and see it was passed (met quorum, and threshold of voters)
        app.update_block(expire(voting_period));
        assert_eq!(prop_status(&app), Status::Rejected);
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
        let (flex_addr, _, _) = setup_test_case(
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
