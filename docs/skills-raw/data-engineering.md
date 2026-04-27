## airflow-dag-patterns

---
name: airflow-dag-patterns
description: Build production Apache Airflow DAGs with best practices for operators, sensors, testing, and deployment. Use when creating data pipelines, orchestrating workflows, or scheduling batch jobs.
---

# Apache Airflow DAG Patterns

Production-ready patterns for Apache Airflow including DAG design, operators, sensors, testing, and deployment strategies.

## When to Use This Skill

- Creating data pipeline orchestration with Airflow
- Designing DAG structures and dependencies
- Implementing custom operators and sensors
- Testing Airflow DAGs locally
- Setting up Airflow in production
- Debugging failed DAG runs

## Core Concepts

### 1. DAG Design Principles

| Principle       | Description                         |
| --------------- | ----------------------------------- |
| **Idempotent**  | Running twice produces same result  |
| **Atomic**      | Tasks succeed or fail completely    |
| **Incremental** | Process only new/changed data       |
| **Observable**  | Logs, metrics, alerts at every step |

### 2. Task Dependencies

```python
# Linear
task1 >> task2 >> task3

# Fan-out
task1 >> [task2, task3, task4]

# Fan-in
[task1, task2, task3] >> task4

# Complex
task1 >> task2 >> task4
task1 >> task3 >> task4
```

## Quick Start

```python
# dags/example_dag.py
from datetime import datetime, timedelta
from airflow import DAG
from airflow.operators.python import PythonOperator
from airflow.operators.empty import EmptyOperator

default_args = {
    'owner': 'data-team',
    'depends_on_past': False,
    'email_on_failure': True,
    'email_on_retry': False,
    'retries': 3,
    'retry_delay': timedelta(minutes=5),
    'retry_exponential_backoff': True,
    'max_retry_delay': timedelta(hours=1),
}

with DAG(
    dag_id='example_etl',
    default_args=default_args,
    description='Example ETL pipeline',
    schedule='0 6 * * *',  # Daily at 6 AM
    start_date=datetime(2024, 1, 1),
    catchup=False,
    tags=['etl', 'example'],
    max_active_runs=1,
) as dag:

    start = EmptyOperator(task_id='start')

    def extract_data(**context):
        execution_date = context['ds']
        # Extract logic here
        return {'records': 1000}

    extract = PythonOperator(
        task_id='extract',
        python_callable=extract_data,
    )

    end = EmptyOperator(task_id='end')

    start >> extract >> end
```

## Patterns

### Pattern 1: TaskFlow API (Airflow 2.0+)

```python
# dags/taskflow_example.py
from datetime import datetime
from airflow.decorators import dag, task
from airflow.models import Variable

@dag(
    dag_id='taskflow_etl',
    schedule='@daily',
    start_date=datetime(2024, 1, 1),
    catchup=False,
    tags=['etl', 'taskflow'],
)
def taskflow_etl():
    """ETL pipeline using TaskFlow API"""

    @task()
    def extract(source: str) -> dict:
        """Extract data from source"""
        import pandas as pd

        df = pd.read_csv(f's3://bucket/{source}/{{ ds }}.csv')
        return {'data': df.to_dict(), 'rows': len(df)}

    @task()
    def transform(extracted: dict) -> dict:
        """Transform extracted data"""
        import pandas as pd

        df = pd.DataFrame(extracted['data'])
        df['processed_at'] = datetime.now()
        df = df.dropna()
        return {'data': df.to_dict(), 'rows': len(df)}

    @task()
    def load(transformed: dict, target: str):
        """Load data to target"""
        import pandas as pd

        df = pd.DataFrame(transformed['data'])
        df.to_parquet(f's3://bucket/{target}/{{ ds }}.parquet')
        return transformed['rows']

    @task()
    def notify(rows_loaded: int):
        """Send notification"""
        print(f'Loaded {rows_loaded} rows')

    # Define dependencies with XCom passing
    extracted = extract(source='raw_data')
    transformed = transform(extracted)
    loaded = load(transformed, target='processed_data')
    notify(loaded)

# Instantiate the DAG
taskflow_etl()
```

### Pattern 2: Dynamic DAG Generation

```python
# dags/dynamic_dag_factory.py
from datetime import datetime, timedelta
from airflow import DAG
from airflow.operators.python import PythonOperator
from airflow.models import Variable
import json

# Configuration for multiple similar pipelines
PIPELINE_CONFIGS = [
    {'name': 'customers', 'schedule': '@daily', 'source': 's3://raw/customers'},
    {'name': 'orders', 'schedule': '@hourly', 'source': 's3://raw/orders'},
    {'name': 'products', 'schedule': '@weekly', 'source': 's3://raw/products'},
]

def create_dag(config: dict) -> DAG:
    """Factory function to create DAGs from config"""

    dag_id = f"etl_{config['name']}"

    default_args = {
        'owner': 'data-team',
        'retries': 3,
        'retry_delay': timedelta(minutes=5),
    }

    dag = DAG(
        dag_id=dag_id,
        default_args=default_args,
        schedule=config['schedule'],
        start_date=datetime(2024, 1, 1),
        catchup=False,
        tags=['etl', 'dynamic', config['name']],
    )

    with dag:
        def extract_fn(source, **context):
            print(f"Extracting from {source} for {context['ds']}")

        def transform_fn(**context):
            print(f"Transforming data for {context['ds']}")

        def load_fn(table_name, **context):
            print(f"Loading to {table_name} for {context['ds']}")

        extract = PythonOperator(
            task_id='extract',
            python_callable=extract_fn,
            op_kwargs={'source': config['source']},
        )

        transform = PythonOperator(
            task_id='transform',
            python_callable=transform_fn,
        )

        load = PythonOperator(
            task_id='load',
            python_callable=load_fn,
            op_kwargs={'table_name': config['name']},
        )

        extract >> transform >> load

    return dag

# Generate DAGs
for config in PIPELINE_CONFIGS:
    globals()[f"dag_{config['name']}"] = create_dag(config)
```

### Pattern 3: Branching and Conditional Logic

```python
# dags/branching_example.py
from airflow.decorators import dag, task
from airflow.operators.python import BranchPythonOperator
from airflow.operators.empty import EmptyOperator
from airflow.utils.trigger_rule import TriggerRule

@dag(
    dag_id='branching_pipeline',
    schedule='@daily',
    start_date=datetime(2024, 1, 1),
    catchup=False,
)
def branching_pipeline():

    @task()
    def check_data_quality() -> dict:
        """Check data quality and return metrics"""
        quality_score = 0.95  # Simulated
        return {'score': quality_score, 'rows': 10000}

    def choose_branch(**context) -> str:
        """Determine which branch to execute"""
        ti = context['ti']
        metrics = ti.xcom_pull(task_ids='check_data_quality')

        if metrics['score'] >= 0.9:
            return 'high_quality_path'
        elif metrics['score'] >= 0.7:
            return 'medium_quality_path'
        else:
            return 'low_quality_path'

    quality_check = check_data_quality()

    branch = BranchPythonOperator(
        task_id='branch',
        python_callable=choose_branch,
    )

    high_quality = EmptyOperator(task_id='high_quality_path')
    medium_quality = EmptyOperator(task_id='medium_quality_path')
    low_quality = EmptyOperator(task_id='low_quality_path')

    # Join point - runs after any branch completes
    join = EmptyOperator(
        task_id='join',
        trigger_rule=TriggerRule.NONE_FAILED_MIN_ONE_SUCCESS,
    )

    quality_check >> branch >> [high_quality, medium_quality, low_quality] >> join

branching_pipeline()
```

### Pattern 4: Sensors and External Dependencies

```python
# dags/sensor_patterns.py
from datetime import datetime, timedelta
from airflow import DAG
from airflow.sensors.filesystem import FileSensor
from airflow.providers.amazon.aws.sensors.s3 import S3KeySensor
from airflow.sensors.external_task import ExternalTaskSensor
from airflow.operators.python import PythonOperator

with DAG(
    dag_id='sensor_example',
    schedule='@daily',
    start_date=datetime(2024, 1, 1),
    catchup=False,
) as dag:

    # Wait for file on S3
    wait_for_file = S3KeySensor(
        task_id='wait_for_s3_file',
        bucket_name='data-lake',
        bucket_key='raw/{{ ds }}/data.parquet',
        aws_conn_id='aws_default',
        timeout=60 * 60 * 2,  # 2 hours
        poke_interval=60 * 5,  # Check every 5 minutes
        mode='reschedule',  # Free up worker slot while waiting
    )

    # Wait for another DAG to complete
    wait_for_upstream = ExternalTaskSensor(
        task_id='wait_for_upstream_dag',
        external_dag_id='upstream_etl',
        external_task_id='final_task',
        execution_date_fn=lambda dt: dt,  # Same execution date
        timeout=60 * 60 * 3,
        mode='reschedule',
    )

    # Custom sensor using @task.sensor decorator
    @task.sensor(poke_interval=60, timeout=3600, mode='reschedule')
    def wait_for_api() -> PokeReturnValue:
        """Custom sensor for API availability"""
        import requests

        response = requests.get('https://api.example.com/health')
        is_done = response.status_code == 200

        return PokeReturnValue(is_done=is_done, xcom_value=response.json())

    api_ready = wait_for_api()

    def process_data(**context):
        api_result = context['ti'].xcom_pull(task_ids='wait_for_api')
        print(f"API returned: {api_result}")

    process = PythonOperator(
        task_id='process',
        python_callable=process_data,
    )

    [wait_for_file, wait_for_upstream, api_ready] >> process
```

### Pattern 5: Error Handling and Alerts

```python
# dags/error_handling.py
from datetime import datetime, timedelta
from airflow import DAG
from airflow.operators.python import PythonOperator
from airflow.utils.trigger_rule import TriggerRule
from airflow.models import Variable

def task_failure_callback(context):
    """Callback on task failure"""
    task_instance = context['task_instance']
    exception = context.get('exception')

    # Send to Slack/PagerDuty/etc
    message = f"""
    Task Failed!
    DAG: {task_instance.dag_id}
    Task: {task_instance.task_id}
    Execution Date: {context['ds']}
    Error: {exception}
    Log URL: {task_instance.log_url}
    """
    # send_slack_alert(message)
    print(message)

def dag_failure_callback(context):
    """Callback on DAG failure"""
    # Aggregate failures, send summary
    pass

with DAG(
    dag_id='error_handling_example',
    schedule='@daily',
    start_date=datetime(2024, 1, 1),
    catchup=False,
    on_failure_callback=dag_failure_callback,
    default_args={
        'on_failure_callback': task_failure_callback,
        'retries': 3,
        'retry_delay': timedelta(minutes=5),
    },
) as dag:

    def might_fail(**context):
        import random
        if random.random() < 0.3:
            raise ValueError("Random failure!")
        return "Success"

    risky_task = PythonOperator(
        task_id='risky_task',
        python_callable=might_fail,
    )

    def cleanup(**context):
        """Cleanup runs regardless of upstream failures"""
        print("Cleaning up...")

    cleanup_task = PythonOperator(
        task_id='cleanup',
        python_callable=cleanup,
        trigger_rule=TriggerRule.ALL_DONE,  # Run even if upstream fails
    )

    def notify_success(**context):
        """Only runs if all upstream succeeded"""
        print("All tasks succeeded!")

    success_notification = PythonOperator(
        task_id='notify_success',
        python_callable=notify_success,
        trigger_rule=TriggerRule.ALL_SUCCESS,
    )

    risky_task >> [cleanup_task, success_notification]
```

### Pattern 6: Testing DAGs

```python
# tests/test_dags.py
import pytest
from datetime import datetime
from airflow.models import DagBag

@pytest.fixture
def dagbag():
    return DagBag(dag_folder='dags/', include_examples=False)

def test_dag_loaded(dagbag):
    """Test that all DAGs load without errors"""
    assert len(dagbag.import_errors) == 0, f"DAG import errors: {dagbag.import_errors}"

def test_dag_structure(dagbag):
    """Test specific DAG structure"""
    dag = dagbag.get_dag('example_etl')

    assert dag is not None
    assert len(dag.tasks) == 3
    assert dag.schedule_interval == '0 6 * * *'

def test_task_dependencies(dagbag):
    """Test task dependencies are correct"""
    dag = dagbag.get_dag('example_etl')

    extract_task = dag.get_task('extract')
    assert 'start' in [t.task_id for t in extract_task.upstream_list]
    assert 'end' in [t.task_id for t in extract_task.downstream_list]

def test_dag_integrity(dagbag):
    """Test DAG has no cycles and is valid"""
    for dag_id, dag in dagbag.dags.items():
        assert dag.test_cycle() is None, f"Cycle detected in {dag_id}"

# Test individual task logic
def test_extract_function():
    """Unit test for extract function"""
    from dags.example_dag import extract_data

    result = extract_data(ds='2024-01-01')
    assert 'records' in result
    assert isinstance(result['records'], int)
```

## Project Structure

```
airflow/
├── dags/
│   ├── __init__.py
│   ├── common/
│   │   ├── __init__.py
│   │   ├── operators.py    # Custom operators
│   │   ├── sensors.py      # Custom sensors
│   │   └── callbacks.py    # Alert callbacks
│   ├── etl/
│   │   ├── customers.py
│   │   └── orders.py
│   └── ml/
│       └── training.py
├── plugins/
│   └── custom_plugin.py
├── tests/
│   ├── __init__.py
│   ├── test_dags.py
│   └── test_operators.py
├── docker-compose.yml
└── requirements.txt
```

## Best Practices

### Do's

- **Use TaskFlow API** - Cleaner code, automatic XCom
- **Set timeouts** - Prevent zombie tasks
- **Use `mode='reschedule'`** - For sensors, free up workers
- **Test DAGs** - Unit tests and integration tests
- **Idempotent tasks** - Safe to retry

### Don'ts

- **Don't use `depends_on_past=True`** - Creates bottlenecks
- **Don't hardcode dates** - Use `{{ ds }}` macros
- **Don't use global state** - Tasks should be stateless
- **Don't skip catchup blindly** - Understand implications
- **Don't put heavy logic in DAG file** - Import from modules

## data-quality-frameworks

---
name: data-quality-frameworks
description: Implement data quality validation with Great Expectations, dbt tests, and data contracts. Use when building data quality pipelines, implementing validation rules, or establishing data contracts.
---

# Data Quality Frameworks

Production patterns for implementing data quality with Great Expectations, dbt tests, and data contracts to ensure reliable data pipelines.

## When to Use This Skill

- Implementing data quality checks in pipelines
- Setting up Great Expectations validation
- Building comprehensive dbt test suites
- Establishing data contracts between teams
- Monitoring data quality metrics
- Automating data validation in CI/CD

## Core Concepts

### 1. Data Quality Dimensions

| Dimension        | Description              | Example Check                                      |
| ---------------- | ------------------------ | -------------------------------------------------- |
| **Completeness** | No missing values        | `expect_column_values_to_not_be_null`              |
| **Uniqueness**   | No duplicates            | `expect_column_values_to_be_unique`                |
| **Validity**     | Values in expected range | `expect_column_values_to_be_in_set`                |
| **Accuracy**     | Data matches reality     | Cross-reference validation                         |
| **Consistency**  | No contradictions        | `expect_column_pair_values_A_to_be_greater_than_B` |
| **Timeliness**   | Data is recent           | `expect_column_max_to_be_between`                  |

### 2. Testing Pyramid for Data

```
          /\
         /  \     Integration Tests (cross-table)
        /────\
       /      \   Unit Tests (single column)
      /────────\
     /          \ Schema Tests (structure)
    /────────────\
```

## Quick Start

### Great Expectations Setup

```bash
# Install
pip install great_expectations

# Initialize project
great_expectations init

# Create datasource
great_expectations datasource new
```

```python
# great_expectations/checkpoints/daily_validation.yml
import great_expectations as gx

# Create context
context = gx.get_context()

# Create expectation suite
suite = context.add_expectation_suite("orders_suite")

# Add expectations
suite.add_expectation(
    gx.expectations.ExpectColumnValuesToNotBeNull(column="order_id")
)
suite.add_expectation(
    gx.expectations.ExpectColumnValuesToBeUnique(column="order_id")
)

# Validate
results = context.run_checkpoint(checkpoint_name="daily_orders")
```

## Patterns

### Pattern 1: Great Expectations Suite

```python
# expectations/orders_suite.py
import great_expectations as gx
from great_expectations.core import ExpectationSuite
from great_expectations.core.expectation_configuration import ExpectationConfiguration

def build_orders_suite() -> ExpectationSuite:
    """Build comprehensive orders expectation suite"""

    suite = ExpectationSuite(expectation_suite_name="orders_suite")

    # Schema expectations
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_table_columns_to_match_set",
        kwargs={
            "column_set": ["order_id", "customer_id", "amount", "status", "created_at"],
            "exact_match": False  # Allow additional columns
        }
    ))

    # Primary key
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_values_to_not_be_null",
        kwargs={"column": "order_id"}
    ))
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_values_to_be_unique",
        kwargs={"column": "order_id"}
    ))

    # Foreign key
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_values_to_not_be_null",
        kwargs={"column": "customer_id"}
    ))

    # Categorical values
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_values_to_be_in_set",
        kwargs={
            "column": "status",
            "value_set": ["pending", "processing", "shipped", "delivered", "cancelled"]
        }
    ))

    # Numeric ranges
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_values_to_be_between",
        kwargs={
            "column": "amount",
            "min_value": 0,
            "max_value": 100000,
            "strict_min": True  # amount > 0
        }
    ))

    # Date validity
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_values_to_be_dateutil_parseable",
        kwargs={"column": "created_at"}
    ))

    # Freshness - data should be recent
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_max_to_be_between",
        kwargs={
            "column": "created_at",
            "min_value": {"$PARAMETER": "now - timedelta(days=1)"},
            "max_value": {"$PARAMETER": "now"}
        }
    ))

    # Row count sanity
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_table_row_count_to_be_between",
        kwargs={
            "min_value": 1000,  # Expect at least 1000 rows
            "max_value": 10000000
        }
    ))

    # Statistical expectations
    suite.add_expectation(ExpectationConfiguration(
        expectation_type="expect_column_mean_to_be_between",
        kwargs={
            "column": "amount",
            "min_value": 50,
            "max_value": 500
        }
    ))

    return suite
```

### Pattern 2: Great Expectations Checkpoint

```yaml
# great_expectations/checkpoints/orders_checkpoint.yml
name: orders_checkpoint
config_version: 1.0
class_name: Checkpoint
run_name_template: "%Y%m%d-%H%M%S-orders-validation"

validations:
  - batch_request:
      datasource_name: warehouse
      data_connector_name: default_inferred_data_connector_name
      data_asset_name: orders
      data_connector_query:
        index: -1 # Latest batch
    expectation_suite_name: orders_suite

action_list:
  - name: store_validation_result
    action:
      class_name: StoreValidationResultAction

  - name: store_evaluation_parameters
    action:
      class_name: StoreEvaluationParametersAction

  - name: update_data_docs
    action:
      class_name: UpdateDataDocsAction

  # Slack notification on failure
  - name: send_slack_notification
    action:
      class_name: SlackNotificationAction
      slack_webhook: ${SLACK_WEBHOOK}
      notify_on: failure
      renderer:
        module_name: great_expectations.render.renderer.slack_renderer
        class_name: SlackRenderer
```

```python
# Run checkpoint
import great_expectations as gx

context = gx.get_context()
result = context.run_checkpoint(checkpoint_name="orders_checkpoint")

if not result.success:
    failed_expectations = [
        r for r in result.run_results.values()
        if not r.success
    ]
    raise ValueError(f"Data quality check failed: {failed_expectations}")
```

### Pattern 3: dbt Data Tests

```yaml
# models/marts/core/_core__models.yml
version: 2

models:
  - name: fct_orders
    description: Order fact table
    tests:
      # Table-level tests
      - dbt_utils.recency:
          datepart: day
          field: created_at
          interval: 1
      - dbt_utils.at_least_one
      - dbt_utils.expression_is_true:
          expression: "total_amount >= 0"

    columns:
      - name: order_id
        description: Primary key
        tests:
          - unique
          - not_null

      - name: customer_id
        description: Foreign key to dim_customers
        tests:
          - not_null
          - relationships:
              to: ref('dim_customers')
              field: customer_id

      - name: order_status
        tests:
          - accepted_values:
              values:
                ["pending", "processing", "shipped", "delivered", "cancelled"]

      - name: total_amount
        tests:
          - not_null
          - dbt_utils.expression_is_true:
              expression: ">= 0"

      - name: created_at
        tests:
          - not_null
          - dbt_utils.expression_is_true:
              expression: "<= current_timestamp"

  - name: dim_customers
    columns:
      - name: customer_id
        tests:
          - unique
          - not_null

      - name: email
        tests:
          - unique
          - not_null
          # Custom regex test
          - dbt_utils.expression_is_true:
              expression: "email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}$'"
```

### Pattern 4: Custom dbt Tests

```sql
-- tests/generic/test_row_count_in_range.sql
{% test row_count_in_range(model, min_count, max_count) %}

with row_count as (
    select count(*) as cnt from {{ model }}
)

select cnt
from row_count
where cnt < {{ min_count }} or cnt > {{ max_count }}

{% endtest %}

-- Usage in schema.yml:
-- tests:
--   - row_count_in_range:
--       min_count: 1000
--       max_count: 10000000
```

```sql
-- tests/generic/test_sequential_values.sql
{% test sequential_values(model, column_name, interval=1) %}

with lagged as (
    select
        {{ column_name }},
        lag({{ column_name }}) over (order by {{ column_name }}) as prev_value
    from {{ model }}
)

select *
from lagged
where {{ column_name }} - prev_value != {{ interval }}
  and prev_value is not null

{% endtest %}
```

```sql
-- tests/singular/assert_orders_customers_match.sql
-- Singular test: specific business rule

with orders_customers as (
    select distinct customer_id from {{ ref('fct_orders') }}
),

dim_customers as (
    select customer_id from {{ ref('dim_customers') }}
),

orphaned_orders as (
    select o.customer_id
    from orders_customers o
    left join dim_customers c using (customer_id)
    where c.customer_id is null
)

select * from orphaned_orders
-- Test passes if this returns 0 rows
```

### Pattern 5: Data Contracts

```yaml
# contracts/orders_contract.yaml
apiVersion: datacontract.com/v1.0.0
kind: DataContract
metadata:
  name: orders
  version: 1.0.0
  owner: data-platform-team
  contact: data-team@company.com

info:
  title: Orders Data Contract
  description: Contract for order event data from the ecommerce platform
  purpose: Analytics, reporting, and ML features

servers:
  production:
    type: snowflake
    account: company.us-east-1
    database: ANALYTICS
    schema: CORE

terms:
  usage: Internal analytics only
  limitations: PII must not be exposed in downstream marts
  billing: Charged per query TB scanned

schema:
  type: object
  properties:
    order_id:
      type: string
      format: uuid
      description: Unique order identifier
      required: true
      unique: true
      pii: false

    customer_id:
      type: string
      format: uuid
      description: Customer identifier
      required: true
      pii: true
      piiClassification: indirect

    total_amount:
      type: number
      minimum: 0
      maximum: 100000
      description: Order total in USD

    created_at:
      type: string
      format: date-time
      description: Order creation timestamp
      required: true

    status:
      type: string
      enum: [pending, processing, shipped, delivered, cancelled]
      description: Current order status

quality:
  type: SodaCL
  specification:
    checks for orders:
      - row_count > 0
      - missing_count(order_id) = 0
      - duplicate_count(order_id) = 0
      - invalid_count(status) = 0:
          valid values: [pending, processing, shipped, delivered, cancelled]
      - freshness(created_at) < 24h

sla:
  availability: 99.9%
  freshness: 1 hour
  latency: 5 minutes
```

### Pattern 6: Automated Quality Pipeline

```python
# quality_pipeline.py
from dataclasses import dataclass
from typing import List, Dict, Any
import great_expectations as gx
from datetime import datetime

@dataclass
class QualityResult:
    table: str
    passed: bool
    total_expectations: int
    failed_expectations: int
    details: List[Dict[str, Any]]
    timestamp: datetime

class DataQualityPipeline:
    """Orchestrate data quality checks across tables"""

    def __init__(self, context: gx.DataContext):
        self.context = context
        self.results: List[QualityResult] = []

    def validate_table(self, table: str, suite: str) -> QualityResult:
        """Validate a single table against expectation suite"""

        checkpoint_config = {
            "name": f"{table}_validation",
            "config_version": 1.0,
            "class_name": "Checkpoint",
            "validations": [{
                "batch_request": {
                    "datasource_name": "warehouse",
                    "data_asset_name": table,
                },
                "expectation_suite_name": suite,
            }],
        }

        result = self.context.run_checkpoint(**checkpoint_config)

        # Parse results
        validation_result = list(result.run_results.values())[0]
        results = validation_result.results

        failed = [r for r in results if not r.success]

        return QualityResult(
            table=table,
            passed=result.success,
            total_expectations=len(results),
            failed_expectations=len(failed),
            details=[{
                "expectation": r.expectation_config.expectation_type,
                "success": r.success,
                "observed_value": r.result.get("observed_value"),
            } for r in results],
            timestamp=datetime.now()
        )

    def run_all(self, tables: Dict[str, str]) -> Dict[str, QualityResult]:
        """Run validation for all tables"""
        results = {}

        for table, suite in tables.items():
            print(f"Validating {table}...")
            results[table] = self.validate_table(table, suite)

        return results

    def generate_report(self, results: Dict[str, QualityResult]) -> str:
        """Generate quality report"""
        report = ["# Data Quality Report", f"Generated: {datetime.now()}", ""]

        total_passed = sum(1 for r in results.values() if r.passed)
        total_tables = len(results)

        report.append(f"## Summary: {total_passed}/{total_tables} tables passed")
        report.append("")

        for table, result in results.items():
            status = "✅" if result.passed else "❌"
            report.append(f"### {status} {table}")
            report.append(f"- Expectations: {result.total_expectations}")
            report.append(f"- Failed: {result.failed_expectations}")

            if not result.passed:
                report.append("- Failed checks:")
                for detail in result.details:
                    if not detail["success"]:
                        report.append(f"  - {detail['expectation']}: {detail['observed_value']}")
            report.append("")

        return "\n".join(report)

# Usage
context = gx.get_context()
pipeline = DataQualityPipeline(context)

tables_to_validate = {
    "orders": "orders_suite",
    "customers": "customers_suite",
    "products": "products_suite",
}

results = pipeline.run_all(tables_to_validate)
report = pipeline.generate_report(results)

# Fail pipeline if any table failed
if not all(r.passed for r in results.values()):
    print(report)
    raise ValueError("Data quality checks failed!")
```

## Best Practices

### Do's

- **Test early** - Validate source data before transformations
- **Test incrementally** - Add tests as you find issues
- **Document expectations** - Clear descriptions for each test
- **Alert on failures** - Integrate with monitoring
- **Version contracts** - Track schema changes

### Don'ts

- **Don't test everything** - Focus on critical columns
- **Don't ignore warnings** - They often precede failures
- **Don't skip freshness** - Stale data is bad data
- **Don't hardcode thresholds** - Use dynamic baselines
- **Don't test in isolation** - Test relationships too

## dbt-transformation-patterns

---
name: dbt-transformation-patterns
description: Master dbt (data build tool) for analytics engineering with model organization, testing, documentation, and incremental strategies. Use when building data transformations, creating data models, or implementing analytics engineering best practices.
---

# dbt Transformation Patterns

Production-ready patterns for dbt (data build tool) including model organization, testing strategies, documentation, and incremental processing.

## When to Use This Skill

- Building data transformation pipelines with dbt
- Organizing models into staging, intermediate, and marts layers
- Implementing data quality tests
- Creating incremental models for large datasets
- Documenting data models and lineage
- Setting up dbt project structure

## Core Concepts

### 1. Model Layers (Medallion Architecture)

```
sources/          Raw data definitions
    ↓
staging/          1:1 with source, light cleaning
    ↓
intermediate/     Business logic, joins, aggregations
    ↓
marts/            Final analytics tables
```

### 2. Naming Conventions

| Layer        | Prefix         | Example                       |
| ------------ | -------------- | ----------------------------- |
| Staging      | `stg_`         | `stg_stripe__payments`        |
| Intermediate | `int_`         | `int_payments_pivoted`        |
| Marts        | `dim_`, `fct_` | `dim_customers`, `fct_orders` |

## Quick Start

```yaml
# dbt_project.yml
name: "analytics"
version: "1.0.0"
profile: "analytics"

model-paths: ["models"]
analysis-paths: ["analyses"]
test-paths: ["tests"]
seed-paths: ["seeds"]
macro-paths: ["macros"]

vars:
  start_date: "2020-01-01"

models:
  analytics:
    staging:
      +materialized: view
      +schema: staging
    intermediate:
      +materialized: ephemeral
    marts:
      +materialized: table
      +schema: analytics
```

```
# Project structure
models/
├── staging/
│   ├── stripe/
│   │   ├── _stripe__sources.yml
│   │   ├── _stripe__models.yml
│   │   ├── stg_stripe__customers.sql
│   │   └── stg_stripe__payments.sql
│   └── shopify/
│       ├── _shopify__sources.yml
│       └── stg_shopify__orders.sql
├── intermediate/
│   └── finance/
│       └── int_payments_pivoted.sql
└── marts/
    ├── core/
    │   ├── _core__models.yml
    │   ├── dim_customers.sql
    │   └── fct_orders.sql
    └── finance/
        └── fct_revenue.sql
```

## Patterns

### Pattern 1: Source Definitions

```yaml
# models/staging/stripe/_stripe__sources.yml
version: 2

sources:
  - name: stripe
    description: Raw Stripe data loaded via Fivetran
    database: raw
    schema: stripe
    loader: fivetran
    loaded_at_field: _fivetran_synced
    freshness:
      warn_after: { count: 12, period: hour }
      error_after: { count: 24, period: hour }
    tables:
      - name: customers
        description: Stripe customer records
        columns:
          - name: id
            description: Primary key
            tests:
              - unique
              - not_null
          - name: email
            description: Customer email
          - name: created
            description: Account creation timestamp

      - name: payments
        description: Stripe payment transactions
        columns:
          - name: id
            tests:
              - unique
              - not_null
          - name: customer_id
            tests:
              - not_null
              - relationships:
                  to: source('stripe', 'customers')
                  field: id
```

### Pattern 2: Staging Models

```sql
-- models/staging/stripe/stg_stripe__customers.sql
with source as (
    select * from {{ source('stripe', 'customers') }}
),

renamed as (
    select
        -- ids
        id as customer_id,

        -- strings
        lower(email) as email,
        name as customer_name,

        -- timestamps
        created as created_at,

        -- metadata
        _fivetran_synced as _loaded_at

    from source
)

select * from renamed
```

```sql
-- models/staging/stripe/stg_stripe__payments.sql
{{
    config(
        materialized='incremental',
        unique_key='payment_id',
        on_schema_change='append_new_columns'
    )
}}

with source as (
    select * from {{ source('stripe', 'payments') }}

    {% if is_incremental() %}
    where _fivetran_synced > (select max(_loaded_at) from {{ this }})
    {% endif %}
),

renamed as (
    select
        -- ids
        id as payment_id,
        customer_id,
        invoice_id,

        -- amounts (convert cents to dollars)
        amount / 100.0 as amount,
        amount_refunded / 100.0 as amount_refunded,

        -- status
        status as payment_status,

        -- timestamps
        created as created_at,

        -- metadata
        _fivetran_synced as _loaded_at

    from source
)

select * from renamed
```

### Pattern 3: Intermediate Models

```sql
-- models/intermediate/finance/int_payments_pivoted_to_customer.sql
with payments as (
    select * from {{ ref('stg_stripe__payments') }}
),

customers as (
    select * from {{ ref('stg_stripe__customers') }}
),

payment_summary as (
    select
        customer_id,
        count(*) as total_payments,
        count(case when payment_status = 'succeeded' then 1 end) as successful_payments,
        sum(case when payment_status = 'succeeded' then amount else 0 end) as total_amount_paid,
        min(created_at) as first_payment_at,
        max(created_at) as last_payment_at
    from payments
    group by customer_id
)

select
    customers.customer_id,
    customers.email,
    customers.created_at as customer_created_at,
    coalesce(payment_summary.total_payments, 0) as total_payments,
    coalesce(payment_summary.successful_payments, 0) as successful_payments,
    coalesce(payment_summary.total_amount_paid, 0) as lifetime_value,
    payment_summary.first_payment_at,
    payment_summary.last_payment_at

from customers
left join payment_summary using (customer_id)
```

### Pattern 4: Mart Models (Dimensions and Facts)

```sql
-- models/marts/core/dim_customers.sql
{{
    config(
        materialized='table',
        unique_key='customer_id'
    )
}}

with customers as (
    select * from {{ ref('int_payments_pivoted_to_customer') }}
),

orders as (
    select * from {{ ref('stg_shopify__orders') }}
),

order_summary as (
    select
        customer_id,
        count(*) as total_orders,
        sum(total_price) as total_order_value,
        min(created_at) as first_order_at,
        max(created_at) as last_order_at
    from orders
    group by customer_id
),

final as (
    select
        -- surrogate key
        {{ dbt_utils.generate_surrogate_key(['customers.customer_id']) }} as customer_key,

        -- natural key
        customers.customer_id,

        -- attributes
        customers.email,
        customers.customer_created_at,

        -- payment metrics
        customers.total_payments,
        customers.successful_payments,
        customers.lifetime_value,
        customers.first_payment_at,
        customers.last_payment_at,

        -- order metrics
        coalesce(order_summary.total_orders, 0) as total_orders,
        coalesce(order_summary.total_order_value, 0) as total_order_value,
        order_summary.first_order_at,
        order_summary.last_order_at,

        -- calculated fields
        case
            when customers.lifetime_value >= 1000 then 'high'
            when customers.lifetime_value >= 100 then 'medium'
            else 'low'
        end as customer_tier,

        -- timestamps
        current_timestamp as _loaded_at

    from customers
    left join order_summary using (customer_id)
)

select * from final
```

```sql
-- models/marts/core/fct_orders.sql
{{
    config(
        materialized='incremental',
        unique_key='order_id',
        incremental_strategy='merge'
    )
}}

with orders as (
    select * from {{ ref('stg_shopify__orders') }}

    {% if is_incremental() %}
    where updated_at > (select max(updated_at) from {{ this }})
    {% endif %}
),

customers as (
    select * from {{ ref('dim_customers') }}
),

final as (
    select
        -- keys
        orders.order_id,
        customers.customer_key,
        orders.customer_id,

        -- dimensions
        orders.order_status,
        orders.fulfillment_status,
        orders.payment_status,

        -- measures
        orders.subtotal,
        orders.tax,
        orders.shipping,
        orders.total_price,
        orders.total_discount,
        orders.item_count,

        -- timestamps
        orders.created_at,
        orders.updated_at,
        orders.fulfilled_at,

        -- metadata
        current_timestamp as _loaded_at

    from orders
    left join customers on orders.customer_id = customers.customer_id
)

select * from final
```

### Pattern 5: Testing and Documentation

```yaml
# models/marts/core/_core__models.yml
version: 2

models:
  - name: dim_customers
    description: Customer dimension with payment and order metrics
    columns:
      - name: customer_key
        description: Surrogate key for the customer dimension
        tests:
          - unique
          - not_null

      - name: customer_id
        description: Natural key from source system
        tests:
          - unique
          - not_null

      - name: email
        description: Customer email address
        tests:
          - not_null

      - name: customer_tier
        description: Customer value tier based on lifetime value
        tests:
          - accepted_values:
              values: ["high", "medium", "low"]

      - name: lifetime_value
        description: Total amount paid by customer
        tests:
          - dbt_utils.expression_is_true:
              expression: ">= 0"

  - name: fct_orders
    description: Order fact table with all order transactions
    tests:
      - dbt_utils.recency:
          datepart: day
          field: created_at
          interval: 1
    columns:
      - name: order_id
        tests:
          - unique
          - not_null
      - name: customer_key
        tests:
          - not_null
          - relationships:
              to: ref('dim_customers')
              field: customer_key
```

### Pattern 6: Macros and DRY Code

```sql
-- macros/cents_to_dollars.sql
{% macro cents_to_dollars(column_name, precision=2) %}
    round({{ column_name }} / 100.0, {{ precision }})
{% endmacro %}

-- macros/generate_schema_name.sql
{% macro generate_schema_name(custom_schema_name, node) %}
    {%- set default_schema = target.schema -%}
    {%- if custom_schema_name is none -%}
        {{ default_schema }}
    {%- else -%}
        {{ default_schema }}_{{ custom_schema_name }}
    {%- endif -%}
{% endmacro %}

-- macros/limit_data_in_dev.sql
{% macro limit_data_in_dev(column_name, days=3) %}
    {% if target.name == 'dev' %}
        where {{ column_name }} >= dateadd(day, -{{ days }}, current_date)
    {% endif %}
{% endmacro %}

-- Usage in model
select * from {{ ref('stg_orders') }}
{{ limit_data_in_dev('created_at') }}
```

### Pattern 7: Incremental Strategies

```sql
-- Delete+Insert (default for most warehouses)
{{
    config(
        materialized='incremental',
        unique_key='id',
        incremental_strategy='delete+insert'
    )
}}

-- Merge (best for late-arriving data)
{{
    config(
        materialized='incremental',
        unique_key='id',
        incremental_strategy='merge',
        merge_update_columns=['status', 'amount', 'updated_at']
    )
}}

-- Insert Overwrite (partition-based)
{{
    config(
        materialized='incremental',
        incremental_strategy='insert_overwrite',
        partition_by={
            "field": "created_date",
            "data_type": "date",
            "granularity": "day"
        }
    )
}}

select
    *,
    date(created_at) as created_date
from {{ ref('stg_events') }}

{% if is_incremental() %}
where created_date >= dateadd(day, -3, current_date)
{% endif %}
```

## dbt Commands

```bash
# Development
dbt run                          # Run all models
dbt run --select staging         # Run staging models only
dbt run --select +fct_orders     # Run fct_orders and its upstream
dbt run --select fct_orders+     # Run fct_orders and its downstream
dbt run --full-refresh           # Rebuild incremental models

# Testing
dbt test                         # Run all tests
dbt test --select stg_stripe     # Test specific models
dbt build                        # Run + test in DAG order

# Documentation
dbt docs generate                # Generate docs
dbt docs serve                   # Serve docs locally

# Debugging
dbt compile                      # Compile SQL without running
dbt debug                        # Test connection
dbt ls --select tag:critical     # List models by tag
```

## Best Practices

### Do's

- **Use staging layer** - Clean data once, use everywhere
- **Test aggressively** - Not null, unique, relationships
- **Document everything** - Column descriptions, model descriptions
- **Use incremental** - For tables > 1M rows
- **Version control** - dbt project in Git

### Don'ts

- **Don't skip staging** - Raw → mart is tech debt
- **Don't hardcode dates** - Use `{{ var('start_date') }}`
- **Don't repeat logic** - Extract to macros
- **Don't test in prod** - Use dev target
- **Don't ignore freshness** - Monitor source data

## spark-optimization

---
name: spark-optimization
description: Optimize Apache Spark jobs with partitioning, caching, shuffle optimization, and memory tuning. Use when improving Spark performance, debugging slow jobs, or scaling data processing pipelines.
---

# Apache Spark Optimization

Production patterns for optimizing Apache Spark jobs including partitioning strategies, memory management, shuffle optimization, and performance tuning.

## When to Use This Skill

- Optimizing slow Spark jobs
- Tuning memory and executor configuration
- Implementing efficient partitioning strategies
- Debugging Spark performance issues
- Scaling Spark pipelines for large datasets
- Reducing shuffle and data skew

## Core Concepts

### 1. Spark Execution Model

```
Driver Program
    ↓
Job (triggered by action)
    ↓
Stages (separated by shuffles)
    ↓
Tasks (one per partition)
```

### 2. Key Performance Factors

| Factor            | Impact                | Solution                      |
| ----------------- | --------------------- | ----------------------------- |
| **Shuffle**       | Network I/O, disk I/O | Minimize wide transformations |
| **Data Skew**     | Uneven task duration  | Salting, broadcast joins      |
| **Serialization** | CPU overhead          | Use Kryo, columnar formats    |
| **Memory**        | GC pressure, spills   | Tune executor memory          |
| **Partitions**    | Parallelism           | Right-size partitions         |

## Quick Start

```python
from pyspark.sql import SparkSession
from pyspark.sql import functions as F

# Create optimized Spark session
spark = (SparkSession.builder
    .appName("OptimizedJob")
    .config("spark.sql.adaptive.enabled", "true")
    .config("spark.sql.adaptive.coalescePartitions.enabled", "true")
    .config("spark.sql.adaptive.skewJoin.enabled", "true")
    .config("spark.serializer", "org.apache.spark.serializer.KryoSerializer")
    .config("spark.sql.shuffle.partitions", "200")
    .getOrCreate())

# Read with optimized settings
df = (spark.read
    .format("parquet")
    .option("mergeSchema", "false")
    .load("s3://bucket/data/"))

# Efficient transformations
result = (df
    .filter(F.col("date") >= "2024-01-01")
    .select("id", "amount", "category")
    .groupBy("category")
    .agg(F.sum("amount").alias("total")))

result.write.mode("overwrite").parquet("s3://bucket/output/")
```

## Patterns

### Pattern 1: Optimal Partitioning

```python
# Calculate optimal partition count
def calculate_partitions(data_size_gb: float, partition_size_mb: int = 128) -> int:
    """
    Optimal partition size: 128MB - 256MB
    Too few: Under-utilization, memory pressure
    Too many: Task scheduling overhead
    """
    return max(int(data_size_gb * 1024 / partition_size_mb), 1)

# Repartition for even distribution
df_repartitioned = df.repartition(200, "partition_key")

# Coalesce to reduce partitions (no shuffle)
df_coalesced = df.coalesce(100)

# Partition pruning with predicate pushdown
df = (spark.read.parquet("s3://bucket/data/")
    .filter(F.col("date") == "2024-01-01"))  # Spark pushes this down

# Write with partitioning for future queries
(df.write
    .partitionBy("year", "month", "day")
    .mode("overwrite")
    .parquet("s3://bucket/partitioned_output/"))
```

### Pattern 2: Join Optimization

```python
from pyspark.sql import functions as F
from pyspark.sql.types import *

# 1. Broadcast Join - Small table joins
# Best when: One side < 10MB (configurable)
small_df = spark.read.parquet("s3://bucket/small_table/")  # < 10MB
large_df = spark.read.parquet("s3://bucket/large_table/")  # TBs

# Explicit broadcast hint
result = large_df.join(
    F.broadcast(small_df),
    on="key",
    how="left"
)

# 2. Sort-Merge Join - Default for large tables
# Requires shuffle, but handles any size
result = large_df1.join(large_df2, on="key", how="inner")

# 3. Bucket Join - Pre-sorted, no shuffle at join time
# Write bucketed tables
(df.write
    .bucketBy(200, "customer_id")
    .sortBy("customer_id")
    .mode("overwrite")
    .saveAsTable("bucketed_orders"))

# Join bucketed tables (no shuffle!)
orders = spark.table("bucketed_orders")
customers = spark.table("bucketed_customers")  # Same bucket count
result = orders.join(customers, on="customer_id")

# 4. Skew Join Handling
# Enable AQE skew join optimization
spark.conf.set("spark.sql.adaptive.skewJoin.enabled", "true")
spark.conf.set("spark.sql.adaptive.skewJoin.skewedPartitionFactor", "5")
spark.conf.set("spark.sql.adaptive.skewJoin.skewedPartitionThresholdInBytes", "256MB")

# Manual salting for severe skew
def salt_join(df_skewed, df_other, key_col, num_salts=10):
    """Add salt to distribute skewed keys"""
    # Add salt to skewed side
    df_salted = df_skewed.withColumn(
        "salt",
        (F.rand() * num_salts).cast("int")
    ).withColumn(
        "salted_key",
        F.concat(F.col(key_col), F.lit("_"), F.col("salt"))
    )

    # Explode other side with all salts
    df_exploded = df_other.crossJoin(
        spark.range(num_salts).withColumnRenamed("id", "salt")
    ).withColumn(
        "salted_key",
        F.concat(F.col(key_col), F.lit("_"), F.col("salt"))
    )

    # Join on salted key
    return df_salted.join(df_exploded, on="salted_key", how="inner")
```

### Pattern 3: Caching and Persistence

```python
from pyspark import StorageLevel

# Cache when reusing DataFrame multiple times
df = spark.read.parquet("s3://bucket/data/")
df_filtered = df.filter(F.col("status") == "active")

# Cache in memory (MEMORY_AND_DISK is default)
df_filtered.cache()

# Or with specific storage level
df_filtered.persist(StorageLevel.MEMORY_AND_DISK_SER)

# Force materialization
df_filtered.count()

# Use in multiple actions
agg1 = df_filtered.groupBy("category").count()
agg2 = df_filtered.groupBy("region").sum("amount")

# Unpersist when done
df_filtered.unpersist()

# Storage levels explained:
# MEMORY_ONLY - Fast, but may not fit
# MEMORY_AND_DISK - Spills to disk if needed (recommended)
# MEMORY_ONLY_SER - Serialized, less memory, more CPU
# DISK_ONLY - When memory is tight
# OFF_HEAP - Tungsten off-heap memory

# Checkpoint for complex lineage
spark.sparkContext.setCheckpointDir("s3://bucket/checkpoints/")
df_complex = (df
    .join(other_df, "key")
    .groupBy("category")
    .agg(F.sum("amount")))
df_complex.checkpoint()  # Breaks lineage, materializes
```

### Pattern 4: Memory Tuning

```python
# Executor memory configuration
# spark-submit --executor-memory 8g --executor-cores 4

# Memory breakdown (8GB executor):
# - spark.memory.fraction = 0.6 (60% = 4.8GB for execution + storage)
#   - spark.memory.storageFraction = 0.5 (50% of 4.8GB = 2.4GB for cache)
#   - Remaining 2.4GB for execution (shuffles, joins, sorts)
# - 40% = 3.2GB for user data structures and internal metadata

spark = (SparkSession.builder
    .config("spark.executor.memory", "8g")
    .config("spark.executor.memoryOverhead", "2g")  # For non-JVM memory
    .config("spark.memory.fraction", "0.6")
    .config("spark.memory.storageFraction", "0.5")
    .config("spark.sql.shuffle.partitions", "200")
    # For memory-intensive operations
    .config("spark.sql.autoBroadcastJoinThreshold", "50MB")
    # Prevent OOM on large shuffles
    .config("spark.sql.files.maxPartitionBytes", "128MB")
    .getOrCreate())

# Monitor memory usage
def print_memory_usage(spark):
    """Print current memory usage"""
    sc = spark.sparkContext
    for executor in sc._jsc.sc().getExecutorMemoryStatus().keySet().toArray():
        mem_status = sc._jsc.sc().getExecutorMemoryStatus().get(executor)
        total = mem_status._1() / (1024**3)
        free = mem_status._2() / (1024**3)
        print(f"{executor}: {total:.2f}GB total, {free:.2f}GB free")
```

### Pattern 5: Shuffle Optimization

```python
# Reduce shuffle data size
spark.conf.set("spark.sql.shuffle.partitions", "auto")  # With AQE
spark.conf.set("spark.shuffle.compress", "true")
spark.conf.set("spark.shuffle.spill.compress", "true")

# Pre-aggregate before shuffle
df_optimized = (df
    # Local aggregation first (combiner)
    .groupBy("key", "partition_col")
    .agg(F.sum("value").alias("partial_sum"))
    # Then global aggregation
    .groupBy("key")
    .agg(F.sum("partial_sum").alias("total")))

# Avoid shuffle with map-side operations
# BAD: Shuffle for each distinct
distinct_count = df.select("category").distinct().count()

# GOOD: Approximate distinct (no shuffle)
approx_count = df.select(F.approx_count_distinct("category")).collect()[0][0]

# Use coalesce instead of repartition when reducing partitions
df_reduced = df.coalesce(10)  # No shuffle

# Optimize shuffle with compression
spark.conf.set("spark.io.compression.codec", "lz4")  # Fast compression
```

### Pattern 6: Data Format Optimization

```python
# Parquet optimizations
(df.write
    .option("compression", "snappy")  # Fast compression
    .option("parquet.block.size", 128 * 1024 * 1024)  # 128MB row groups
    .parquet("s3://bucket/output/"))

# Column pruning - only read needed columns
df = (spark.read.parquet("s3://bucket/data/")
    .select("id", "amount", "date"))  # Spark only reads these columns

# Predicate pushdown - filter at storage level
df = (spark.read.parquet("s3://bucket/partitioned/year=2024/")
    .filter(F.col("status") == "active"))  # Pushed to Parquet reader

# Delta Lake optimizations
(df.write
    .format("delta")
    .option("optimizeWrite", "true")  # Bin-packing
    .option("autoCompact", "true")  # Compact small files
    .mode("overwrite")
    .save("s3://bucket/delta_table/"))

# Z-ordering for multi-dimensional queries
spark.sql("""
    OPTIMIZE delta.`s3://bucket/delta_table/`
    ZORDER BY (customer_id, date)
""")
```

### Pattern 7: Monitoring and Debugging

```python
# Enable detailed metrics
spark.conf.set("spark.sql.codegen.wholeStage", "true")
spark.conf.set("spark.sql.execution.arrow.pyspark.enabled", "true")

# Explain query plan
df.explain(mode="extended")
# Modes: simple, extended, codegen, cost, formatted

# Get physical plan statistics
df.explain(mode="cost")

# Monitor task metrics
def analyze_stage_metrics(spark):
    """Analyze recent stage metrics"""
    status_tracker = spark.sparkContext.statusTracker()

    for stage_id in status_tracker.getActiveStageIds():
        stage_info = status_tracker.getStageInfo(stage_id)
        print(f"Stage {stage_id}:")
        print(f"  Tasks: {stage_info.numTasks}")
        print(f"  Completed: {stage_info.numCompletedTasks}")
        print(f"  Failed: {stage_info.numFailedTasks}")

# Identify data skew
def check_partition_skew(df):
    """Check for partition skew"""
    partition_counts = (df
        .withColumn("partition_id", F.spark_partition_id())
        .groupBy("partition_id")
        .count()
        .orderBy(F.desc("count")))

    partition_counts.show(20)

    stats = partition_counts.select(
        F.min("count").alias("min"),
        F.max("count").alias("max"),
        F.avg("count").alias("avg"),
        F.stddev("count").alias("stddev")
    ).collect()[0]

    skew_ratio = stats["max"] / stats["avg"]
    print(f"Skew ratio: {skew_ratio:.2f}x (>2x indicates skew)")
```

## Configuration Cheat Sheet

```python
# Production configuration template
spark_configs = {
    # Adaptive Query Execution (AQE)
    "spark.sql.adaptive.enabled": "true",
    "spark.sql.adaptive.coalescePartitions.enabled": "true",
    "spark.sql.adaptive.skewJoin.enabled": "true",

    # Memory
    "spark.executor.memory": "8g",
    "spark.executor.memoryOverhead": "2g",
    "spark.memory.fraction": "0.6",
    "spark.memory.storageFraction": "0.5",

    # Parallelism
    "spark.sql.shuffle.partitions": "200",
    "spark.default.parallelism": "200",

    # Serialization
    "spark.serializer": "org.apache.spark.serializer.KryoSerializer",
    "spark.sql.execution.arrow.pyspark.enabled": "true",

    # Compression
    "spark.io.compression.codec": "lz4",
    "spark.shuffle.compress": "true",

    # Broadcast
    "spark.sql.autoBroadcastJoinThreshold": "50MB",

    # File handling
    "spark.sql.files.maxPartitionBytes": "128MB",
    "spark.sql.files.openCostInBytes": "4MB",
}
```

## Best Practices

### Do's

- **Enable AQE** - Adaptive query execution handles many issues
- **Use Parquet/Delta** - Columnar formats with compression
- **Broadcast small tables** - Avoid shuffle for small joins
- **Monitor Spark UI** - Check for skew, spills, GC
- **Right-size partitions** - 128MB - 256MB per partition

### Don'ts

- **Don't collect large data** - Keep data distributed
- **Don't use UDFs unnecessarily** - Use built-in functions
- **Don't over-cache** - Memory is limited
- **Don't ignore data skew** - It dominates job time
- **Don't use `.count()` for existence** - Use `.take(1)` or `.isEmpty()`

